//! Sepulcra's shell: the title screen, the player's saved settings, accessibility, rebinding,
//! gamepad pause, subtitles and captions (docs/design/ACCESSIBILITY.md). A child of the game
//! module (like `saves`), so it reaches the game's own fields; the game calls into it from a
//! handful of places.
//!
//! - **Whose settings.** The engine keeps the file per user (`specula::user_settings`). Changes
//!   the player makes with the window's own input are saved; changes an agent makes (a `play_*`
//!   call, `ui_act`'s synthetic frames) apply for the session only, unless the operator named the
//!   settings folder (`--settings DIR`: scripts and tests). [`Shell::saved`] is the player's
//!   file, [`Shell::settings`] what is live.
//! - **The title screen** holds the run like the intro (the simulation isn't stepped) until the
//!   player starts or continues; an agent's acting call takes it away, so agents are never held.
//! - **Rebinding** listens for the window's next real input. An agent can't make the game listen
//!   (it would read the player's next key): its press is refused, and `play_settings {bind}` sets
//!   bindings by name instead.
//! - **Gamepad pause.** Start opens the pause menu and closes it again; a pad that was in use and
//!   disconnects pauses the run with the menu open.

use super::*;
use crate::l10n::Said;
use specula::game::{Frame, GameRequest, GameWorld};
use crate::settings::{Settings, SubtitleSize, Vision, TEXT_SIZES, UI_SCALES as SCALE_OPTIONS, VOLUMES};
use specula::game::SettingsStore;
use specula_input::{Binding, BindingSlot, DeviceFamily, HoldMode, RebindOutcome, RebindRequest, Source};
use specula_platform::MAX_GAMEPADS;
use specula_ui::vision::{self, Deficiency};

/// Captions shown at once, and for how long (presentation seconds).
const MAX_CAPTIONS: usize = 3;
const CAPTION_S: f32 = 3.5;
/// The same sound captioned again within this long counts up instead ("x3").
const CAPTION_MERGE_S: f32 = 2.0;
/// Plays read from the audio log a frame, at most.
const MAX_PLAYS_READ: usize = 64;
/// The caption table: sound name → words.
const CAPTIONS: &str = include_str!("../../sepulcra/assets/captions.json");
/// Most caption table entries read.
const MAX_CAPTION_ENTRIES: usize = 256;
/// The actions the camera-drag setting switches between hold and toggle.
const DRAG_ACTIONS: [&str; 2] = ["orbit", "pan"];

/// The player's ways into the menu (Escape, Start): agents' `play_settings {bind}` never moves
/// them (review R1 of m5-a11y, finding 7). The player rebinds them on the Controls screen, where
/// Reset stays a click away.
const ESSENTIAL_ACTIONS: [&str; 2] = ["cancel", "pause"];
/// Bindings `play_settings {bind}` changes in one call.
pub const MAX_BINDS: usize = 16;
/// The colour pairs that carry meaning (the player's magic, the dead, warnings), and how far
/// apart each colour theme keeps them for the deficiency it is for.
pub const MEANING_PAIRS: [(&str, &str); 3] = [("rune", "grave"), ("rune", "fire"), ("grave", "fire")];
pub const MIN_MEANING_DELTA_E: f32 = 30.0;

/// The shell's state (one field of the game).
#[derive(Default)]
pub(crate) struct Shell {
    /// What is live.
    pub settings: Settings,
    /// The player's file: what is saved.
    pub saved: Settings,
    pub store: SettingsStore,
    /// What in the file couldn't be used.
    pub problems: Vec<String>,
    /// The saved settings changed since the engine last took them.
    dirty: bool,
    /// The input being handled is the window's own (a real frame): its changes are the player's.
    pub human: bool,
    /// `-- --title off`.
    pub title_off: bool,
    /// The title screen holds the run.
    pub title: bool,
    /// The player pressed Quit.
    quit: bool,
    /// Which pads were connected on the last real frame.
    pads: [bool; MAX_GAMEPADS],
    /// Listening for the player's next input: (action, device family, binding slot).
    rebinding: Option<(String, DeviceFamily)>,
    pub rebind_status: Option<Said>,
    /// The subtitle line on screen: (text, speaker, until).
    subtitle: Option<(Said, Option<Said>, f32)>,
    /// Captions on screen: (sound, what it says once, until, count).
    captions: std::collections::VecDeque<(String, Said, f32, u32)>,
    /// The audio log's cursor, and scratch for reading it.
    caption_seq: u64,
    plays: Vec<specula::audio_service::PlayRecord>,
    /// Sound name → the string-table key of its caption's words.
    caption_table: Vec<(String, String)>,
    /// Captions made (play_state).
    pub captioned: u64,
    /// The handheld the display is on, if any (play_state).
    pub handheld: Option<&'static str>,
    /// Refusals of agents' presses that only the player may make (play_state).
    pub refused: u32,
    /// How many of them the engine was told about (`Game::take_refused_presses`).
    pub refused_told: u32,
    /// A camera drag is latched on (camera drag: toggle), and the player was told how to stop
    /// it once this run.
    drag_latched: bool,
    drag_hint_shown: bool,
    /// A menu was open when this frame's input arrived (Start then closes it).
    menu_was_open: bool,
    /// Bumped when the settings or the display change: the screens' words are made again.
    rev: u64,
    texts_key: Option<(u64, specula_ui::Display, u32, usize, Option<super::saves::Slot>)>,
    texts: Vec<(String, Said)>,
    flags: Vec<(String, bool)>,
}

/// The menus the pause menu leads to (Start closes any of them).
const MENUS: [&str; 5] = ["pause", "settings", "controls", "saves", "access"];

/// Reads the caption table (`{"sounds": {name: "caption.key"}}`: each captioned sound and the
/// string-table key of its words), at most [`MAX_CAPTION_ENTRIES`].
pub fn caption_table(text: &str) -> Result<Vec<(String, String)>, String> {
    let j = Json::parse(text).map_err(|e| format!("captions.json: {}", e.message))?;
    let sounds = j.get("sounds").and_then(Json::as_object).ok_or("captions.json: \"sounds\" must be an object of sound name to the string-table key of its words")?;
    Ok(sounds.iter().take(MAX_CAPTION_ENTRIES).filter_map(|(k, v)| v.as_str().map(|t| (k.clone(), t.chars().take(80).collect()))).collect())
}

/// Where a sound at `at` is for a listener with `camera`: left, right, behind, or nothing (in
/// front, or not positioned).
pub fn direction(camera: &Camera, at: [f32; 3]) -> Option<&'static str> {
    let p = Vec3::new(at[0], at[1], at[2]);
    let fwd = camera.target - camera.eye;
    let to = p - camera.eye;
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    let (f, t) = (flat(fwd), flat(to));
    if f.length() < 1e-4 || t.length() < 1e-4 {
        return None;
    }
    let (f, t) = (f * (1.0 / f.length()), t * (1.0 / t.length()));
    let right = Vec3::new(-f.z, 0.0, f.x);
    let ahead = f.x * t.x + f.z * t.z;
    let side = right.x * t.x + right.z * t.z;
    if ahead < -0.5 {
        Some("behind")
    } else if side > 0.5 {
        Some("right")
    } else if side < -0.5 {
        Some("left")
    } else {
        None
    }
}

/// The family a binding-row button rebinds.
fn family_of(device: &str) -> Option<DeviceFamily> {
    match device {
        "kbm" => Some(DeviceFamily::KeyboardMouse),
        "pad" => Some(DeviceFamily::Gamepad),
        _ => None,
    }
}

/// Where a new binding of `family` goes for `action`: over its first binding of that family,
/// else after its others.
fn slot_for(map: &specula_input::ActionMap, action: &str, family: DeviceFamily) -> BindingSlot {
    map.action(action).and_then(|a| a.bindings.iter().position(|b| b.family() == family)).map_or(BindingSlot::Append, BindingSlot::Replace)
}

/// The actions whose bindings the controls screen lists (not the UI's own), in map order.
fn listed(map: &specula_input::ActionMap) -> Vec<&specula_input::Action> {
    map.actions().iter().filter(|a| !a.name.starts_with("ui_")).take(24).collect()
}

/// Other actions (not the UI's) on the same input as one of `action`'s bindings.
fn clashes(map: &specula_input::ActionMap, action: &str) -> Vec<String> {
    let Some(a) = map.action(action) else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for other in map.actions().iter().filter(|o| o.name != action && !o.name.starts_with("ui_")) {
        if a.bindings.iter().any(|b| other.bindings.iter().any(|c| b.same_trigger(c))) && !out.contains(&other.name) {
            out.push(other.name.clone());
        }
    }
    out
}

/// The string-table entry for a switch's state.
fn on_off(b: bool) -> Said {
    Said::new(if b { "common.on" } else { "common.off" })
}

/// A subtitle's text or speaker as a message: `@key` is a string-table entry, anything else is
/// shown as written (at most `max` characters).
fn subtitle_words(s: &str, max: usize) -> Said {
    match s.strip_prefix('@').filter(|k| !k.starts_with('@')) {
        Some(key) => Said::new(key),
        None => Said::new("common.text").text("text", s.chars().take(max).collect::<String>()),
    }
}

impl SepulcraGame {
    // ---- settings ----

    /// The engine read the player's settings (before start).
    pub(crate) fn shell_use_settings(&mut self, text: Option<&str>, store: &SettingsStore) {
        self.shell.store = store.clone();
        if let Some(p) = &store.problem {
            self.shell.problems.push(p.clone());
        }
        if let Some(t) = text {
            let parsed = Settings::parse(t);
            self.shell.problems.extend(parsed.problems);
            self.shell.settings = parsed.settings.clone();
            self.shell.saved = parsed.settings;
        }
    }

    /// Changes a setting: live always; saved too when the player made it (or the operator's
    /// folder saves agents' changes). Then applies what is live.
    ///
    /// `f` **sets** values; it never works one out from the settings it is given. Live and saved
    /// differ after an agent's session-only change, so a toggle run on each would save the
    /// opposite of what the player sees (review R1 of m5-a11y, finding 3). Work the new value
    /// out from what is live (what the player is looking at), then set it. Debug builds check.
    pub(crate) fn change_settings(&mut self, f: impl Fn(&mut Settings)) {
        #[cfg(debug_assertions)]
        {
            let mut once = self.shell.settings.clone();
            f(&mut once);
            let mut twice = once.clone();
            f(&mut twice);
            assert!(once == twice, "change_settings: this change reads the settings it is given (a toggle or a cycle); work the value out from the live settings first");
        }
        self.shell.rev += 1;
        f(&mut self.shell.settings);
        if self.shell.human || self.shell.store.agents_save {
            f(&mut self.shell.saved);
            self.shell.dirty = true;
        }
        self.apply_settings();
    }

    /// The settings file to save, when the saved settings changed.
    pub(crate) fn take_settings_text(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.shell.dirty) || !self.shell.store.saving() {
            return None;
        }
        Some(self.shell.saved.to_text())
    }

    /// The UI scale the interface lays out at: the player's choice, else what the display
    /// suits (a handheld's legible default, else 1).
    pub(crate) fn effective_ui_scale(&self) -> f32 {
        match self.shell.settings.ui_scale {
            Some(i) => SCALE_OPTIONS[i.min(SCALE_OPTIONS.len() - 1)],
            None => self.auto_ui_scale(),
        }
    }

    fn auto_ui_scale(&self) -> f32 {
        self.hud.theme().and_then(|t| self.hud.display().handheld_ui_scale(t, &SCALE_OPTIONS)).unwrap_or(1.0)
    }

    /// Makes what is live take effect: interface and text size, colours, motion, sound, prompts,
    /// autosave, camera drag and bindings.
    pub(crate) fn apply_settings(&mut self) {
        let s = self.shell.settings.clone();
        self.hud.user_scale = self.effective_ui_scale();
        self.ui_scale = s.ui_scale.unwrap_or_else(|| SCALE_OPTIONS.iter().position(|&o| (o - self.hud.user_scale).abs() < 1e-3).unwrap_or(2));
        self.volume = s.volume();
        self.sfx.set_volume(self.volume);
        self.reduced_motion = s.reduced_motion;
        if let Some(a) = s.autosave {
            self.autosave_on = a;
        }
        // Button prompts: "Auto" is the style the input had before any override.
        let input = self.controls.input_mut();
        let auto = *self.auto_glyphs.get_or_insert(input.pad_style());
        self.glyphs = s.glyphs.min(GLYPHS.len() - 1);
        input.set_pad_style(GLYPHS[self.glyphs].1.unwrap_or(auto));
        // The player's bindings over the game's own; a file that doesn't load keeps the game's.
        let mut map = crate::controls::bindings().expect("the built-in bindings are valid (tested)");
        if let Some(b) = &s.bindings {
            if let Err(e) = map.load_bindings_json(b) {
                let why = format!("bindings: {e} (the game's own used)");
                if !self.shell.problems.contains(&why) {
                    self.shell.problems.push(why);
                }
                map = crate::controls::bindings().expect("the built-in bindings are valid (tested)");
            }
        }
        if *input.map() != map && !input.rebinding() {
            input.set_map(map);
        }
        for a in DRAG_ACTIONS {
            let _ = input.set_hold_mode(a, s.camera_drag);
        }
        let look = crate::hud::Look { text_size: s.text_size(), subtitle_size: s.subtitle_size.factor(), vision: s.vision.name(), reduced_motion: s.reduced_motion };
        if let Err(e) = self.hud.set_look(&look) {
            self.say(ToastKind::Warn, Said::new("toast.settings_problem").text("why", e));
        }
    }

    /// The display changed: "Auto" sizes follow it, and the options say what each gives.
    pub(crate) fn shell_display_changed(&mut self) {
        self.shell.rev += 1;
        self.shell.handheld = self.hud.display().handheld().map(|h| h.name);
        self.hud.user_scale = self.effective_ui_scale();
    }

    /// The settings and shell actions (from the HUD's buttons, a player's or an agent's press).
    /// False when `action` isn't one of them.
    pub(crate) fn shell_action(&mut self, world: &mut GameWorld<'_>, agent: &str, action: &str) -> bool {
        let (verb, arg) = action.split_once(':').unwrap_or((action, ""));
        // What is live now: what the player (or the agent) is looking at.
        let live = self.shell.settings.clone();
        match verb {
            "cycle_ui_scale" => {
                // From Auto to the next size up from what Auto gives; sizes that change nothing
                // on this display are passed over (the access screen shows them greyed).
                let now = self.shell.settings.ui_scale.unwrap_or_else(|| SCALE_OPTIONS.iter().position(|&o| o >= self.hud.user_scale - 1e-3).unwrap_or(2));
                let ok = self.scale_options().iter().map(|o| o.limited.is_none()).collect::<Vec<_>>();
                let mut next = (now + 1) % SCALE_OPTIONS.len();
                for _ in 0..SCALE_OPTIONS.len() {
                    if ok.get(next).copied().unwrap_or(true) {
                        break;
                    }
                    next = (next + 1) % SCALE_OPTIONS.len();
                }
                self.change_settings(|s| s.ui_scale = Some(next));
            }
            "set_ui_scale" => {
                let pick = if arg == "auto" { Some(None) } else { arg.parse::<usize>().ok().filter(|&i| i < SCALE_OPTIONS.len()).map(Some) };
                if let Some(v) = pick {
                    self.change_settings(|s| s.ui_scale = v);
                }
            }
            // Each new value is worked out from what is live (what the player sees), then set.
            "cycle_text_size" => {
                let v = Some((live.text_size.unwrap_or(1) + 1) % TEXT_SIZES.len());
                self.change_settings(|s| s.text_size = v);
            }
            "cycle_volume" => {
                let v = (live.volume + 1) % VOLUMES.len();
                self.change_settings(|s| s.volume = v);
            }
            "cycle_glyphs" => {
                let v = (live.glyphs + 1) % GLYPHS.len();
                self.change_settings(|s| s.glyphs = v);
            }
            "toggle_motion" => self.change_settings(|s| s.reduced_motion = !live.reduced_motion),
            "toggle_autosave" => {
                let on = !self.autosave_on;
                self.change_settings(|s| s.autosave = Some(on));
            }
            "cycle_vision" => {
                let i = Vision::ALL.iter().position(|v| *v == live.vision).unwrap_or(0);
                let v = Vision::ALL[(i + 1) % Vision::ALL.len()];
                self.change_settings(|s| s.vision = v);
            }
            "toggle_subtitles" => self.change_settings(|s| s.subtitles = !live.subtitles),
            "toggle_captions" => self.change_settings(|s| s.captions = !live.captions),
            "cycle_subtitle_size" => {
                let i = SubtitleSize::ALL.iter().position(|v| *v == live.subtitle_size).unwrap_or(1);
                let v = SubtitleSize::ALL[(i + 1) % SubtitleSize::ALL.len()];
                self.change_settings(|s| s.subtitle_size = v);
            }
            "toggle_subtitle_bg" => self.change_settings(|s| s.subtitle_background = !live.subtitle_background),
            "toggle_camera_drag" => {
                let v = if live.camera_drag == HoldMode::Hold { HoldMode::Toggle } else { HoldMode::Hold };
                self.change_settings(|s| s.camera_drag = v);
            }
            "open_access" => {
                self.hud.show("settings", false);
                self.hud.show("access", true);
            }
            "close_access" => {
                self.hud.show("access", false);
                self.hud.show("settings", true);
            }
            "close_settings" => {
                self.hud.show("settings", false);
                // Back to the title when it opened them.
                if self.shell.title {
                    self.hud.show("title", true);
                }
            }
            "close_controls" => {
                self.cancel_rebind();
                self.hud.show("controls", false);
                if self.shell.title {
                    self.hud.show("title", true);
                }
            }
            "open_settings" | "open_controls" if self.shell.title => {
                self.hud.show("title", false);
                self.hud.show(if verb == "open_settings" { "settings" } else { "controls" }, true);
            }
            "rebind_kbm" | "rebind_pad" => {
                let row = arg.parse::<usize>().ok();
                self.start_rebind(row, if verb == "rebind_kbm" { "kbm" } else { "pad" });
            }
            "reset_bindings" => {
                self.cancel_rebind();
                self.change_settings(|s| s.bindings = None);
                self.shell.rebind_status = Some(Said::new("rebind.reset"));
            }
            "title_start" => self.close_title(world, agent, false),
            "title_continue" => self.close_title(world, agent, true),
            "quit_game" => {
                if self.shell.human {
                    self.shell.quit = true;
                } else {
                    self.shell.refused += 1;
                    self.say(ToastKind::Info, Said::new("toast.quit_is_players"));
                }
            }
            _ => return false,
        }
        true
    }

    /// Each UI-scale setting on the current display.
    pub(crate) fn scale_options(&self) -> Vec<specula_ui::ScaleOption> {
        match self.hud.theme() {
            Some(t) => self.hud.display().scale_options(t, &SCALE_OPTIONS),
            None => SCALE_OPTIONS.iter().map(|&u| specula_ui::ScaleOption { user: u, effective: u, limited: None }).collect(),
        }
    }

    // ---- the title ----

    /// At start: the title (windowed, unless `--title off`), else the intro straight away.
    pub(crate) fn shell_start(&mut self, windowed: bool) {
        self.shell.caption_table = caption_table(CAPTIONS).unwrap_or_default();
        if let Some(dir) = &self.assets {
            // Start-up only (like the screens and sounds), at most 64 KiB.
            let small = std::fs::metadata(dir.join("captions.json")).is_ok_and(|m| m.len() <= 64 << 10);
            if let Some(t) = small.then(|| std::fs::read_to_string(dir.join("captions.json")).ok()).flatten() {
                match caption_table(&t) {
                    Ok(c) => self.shell.caption_table = c,
                    Err(e) => self.shell.problems.push(e),
                }
            }
        }
        self.apply_settings();
        self.shell_display_changed();
        if windowed && !self.shell.title_off && self.hud.enabled() {
            self.shell.title = true;
            self.hud.show("hud", false);
            self.hud.show("title", true);
            // Continue needs the list of saves.
            self.request(GameRequest::List);
        } else {
            self.cutscene.intro(windowed, self.reduced_motion);
        }
    }

    /// Whether the title holds the run (no steps).
    pub(crate) fn shell_holds(&self) -> bool {
        self.shell.title
    }

    /// The title closes: play (the intro, then wave 1), or load the newest save.
    fn close_title(&mut self, _world: &mut GameWorld<'_>, _agent: &str, resume: bool) {
        if !self.shell.title {
            return;
        }
        if resume {
            let Some(slot) = self.slots.iter().find(|s| s.title.key != "saves.damaged").map(|s| s.slot) else {
                self.say(ToastKind::Info, Said::new("toast.no_save_to_continue"));
                return;
            };
            self.load_from(slot);
        }
        self.shell.title = false;
        for s in ["title", "settings", "controls", "saves", "access"] {
            self.hud.show(s, false);
        }
        self.hud.show("hud", true);
        if !resume {
            self.cutscene.intro(true, self.reduced_motion);
        }
    }

    /// An agent acted on the run (a `play_*` call): the title never holds it. `pressing` is the
    /// widget a `play_ui` call presses: one of the title's own keeps it.
    pub(crate) fn shell_agent_acted(&mut self, pressing: Option<&str>) {
        if !self.shell.title {
            return;
        }
        let on_title = pressing.is_some_and(|id| ["start", "title_continue", "title_settings", "title_controls", "quit_game"].contains(&id))
            || self.hud.visible("settings")
            || self.hud.visible("controls")
            || self.hud.visible("access")
            || self.hud.visible("saves");
        if pressing.is_some() && on_title {
            return;
        }
        self.shell.title = false;
        for s in ["title", "settings", "controls", "saves", "access"] {
            self.hud.show(s, false);
        }
        self.hud.show("hud", true);
    }

    pub(crate) fn take_quit_asked(&mut self) -> bool {
        std::mem::take(&mut self.shell.quit)
    }

    // ---- frames: provenance, gamepad pause, rebinding, captions ----

    /// The start of a frame's input handling (after the controls read it, before the HUD and
    /// the game act on it).
    pub(crate) fn shell_frame(&mut self, world: &mut GameWorld<'_>, frame: &Frame) {
        self.shell.human = !frame.synthetic;
        self.shell.menu_was_open = !self.shell.title && MENUS.iter().any(|m| self.hud.visible(m)) && !self.hud.visible("confirm");
        // Synthetic frames (ui_act's) carry only the pads their steps name: they never count as a
        // pad going away. They end no rebind either: while one listens the input layer drops them
        // whole (`Input::update_synthetic`), so only the window's own input binds or cancels.
        if frame.synthetic {
            return;
        }
        // A pad that was in use went away: pause with the menu open (the player can't act).
        let mut lost = false;
        for i in 0..MAX_GAMEPADS {
            let now = frame.pads.get(i).is_some_and(|p| p.connected);
            if self.shell.pads[i] && !now && matches!(self.controls.input().last_device(), Some(specula_input::Device::Gamepad(_))) {
                lost = true;
            }
            self.shell.pads[i] = now;
        }
        // A camera drag toggled on: say once how it stops (nothing on screen shows the latch).
        let latched = DRAG_ACTIONS.iter().any(|a| self.controls.input().latched(a));
        if latched && !self.shell.drag_latched && !std::mem::replace(&mut self.shell.drag_hint_shown, true) {
            self.say(ToastKind::Info, Said::new("toast.drag_latched"));
        }
        self.shell.drag_latched = latched;
        if lost && self.running() && !self.shell.title && !self.hud.visible("pause") {
            self.open_menu(world);
            self.say(ToastKind::Warn, Said::new("toast.pad_lost"));
            self.shell.menu_was_open = false;
        }
        self.rebind_poll();
    }

    /// The end of a frame's input handling (after the game acted): Start on the open pause menu
    /// (or a screen opened from it) resumes, as Start opened it. The menu is modal, so the same
    /// press reached nothing else this frame. Agents' synthetic frames (ui_act) do the same, as a
    /// player's would.
    pub(crate) fn shell_frame_end(&mut self, world: &mut GameWorld<'_>, _frame: &Frame) {
        if !std::mem::take(&mut self.shell.menu_was_open) {
            return;
        }
        let pad = matches!(self.controls.input().last_device(), Some(specula_input::Device::Gamepad(_)));
        if pad && self.controls.input().pressed("pause") && self.shell.rebinding.is_none() {
            for m in MENUS {
                self.hud.show(m, false);
            }
            self.set_paused(world, false);
        }
    }

    /// Starts listening for the player's next input for row `row` of the controls list.
    fn start_rebind(&mut self, row: Option<usize>, device: &str) {
        if !self.shell.human {
            // Listening would hand an agent the player's next key: refused.
            self.shell.refused += 1;
            self.shell.rebind_status = Some(Said::new("rebind.agents_refused"));
            return;
        }
        let Some(family) = family_of(device) else { return };
        let input = self.controls.input_mut();
        let Some(action) = row.and_then(|r| listed(input.map()).get(r).map(|a| (a.name.clone(), a.description.clone(), a.kind))) else { return };
        if action.2 != specula_input::ActionKind::Button {
            self.shell.rebind_status = Some(Said::new("rebind.only_buttons"));
            return;
        }
        let slot = slot_for(input.map(), &action.0, family);
        let request = RebindRequest { action: action.0.clone(), slot, part: None, family: Some(family) };
        match input.start_rebind(request) {
            Ok(()) => {
                let how = if family == DeviceFamily::Gamepad { "rebind.press_pad" } else { "rebind.press_kbm" };
                self.shell.rebind_status = Some(Said::new(how).key("what", format!("action.{}", action.0)));
                self.shell.rebinding = Some((action.0, family));
            }
            Err(e) => self.shell.rebind_status = Some(Said::new("rebind.cant").text("why", e.to_string())),
        }
    }

    /// The player's own bindings: the game's with their saved ones over them (a saved set that
    /// doesn't load leaves the game's).
    fn players_map(&self) -> specula_input::ActionMap {
        let mut map = crate::controls::bindings().expect("the built-in bindings are valid (tested)");
        if let Some(b) = &self.shell.saved.bindings {
            if map.load_bindings_json(b).is_err() {
                map = crate::controls::bindings().expect("the built-in bindings are valid (tested)");
            }
        }
        map
    }

    fn cancel_rebind(&mut self) {
        if self.shell.rebinding.take().is_some() {
            self.controls.input_mut().cancel_rebind();
            self.shell.rebind_status = None;
        }
    }

    /// A rebind that ended this frame: applied (conflicts are shown, not refused) and saved.
    fn rebind_poll(&mut self) {
        let Some(outcome) = self.controls.input_mut().take_rebind_outcome() else { return };
        self.shell.rebinding = None;
        match &outcome {
            RebindOutcome::Cancelled { .. } => self.shell.rebind_status = Some(Said::new("rebind.cancelled")),
            RebindOutcome::Captured { request, binding, .. } => {
                let glyph = binding.glyph(self.controls.input().pad_style());
                match self.controls.input_mut().apply_rebind(&outcome) {
                    Ok(()) => {
                        let clash = self.action_names(&clashes(self.controls.input().map(), &request.action));
                        self.shell.rebind_status = Some(if clash.is_empty() {
                            Said::new("rebind.bound").text("glyph", glyph)
                        } else {
                            Said::new("rebind.bound_clash").text("glyph", glyph).text("others", clash)
                        });
                        // Live: the map as it is now (with whatever an agent bound for the
                        // session). Saved: the player's own bindings with this one made, so an
                        // agent's session-only binding never rides into the player's file
                        // (review R1 of m5-a11y, finding 2).
                        let live = self.controls.input().map().bindings_json();
                        let mut mine = self.players_map();
                        // The slot is worked out in the player's own map: the live one may hold
                        // an agent's binding the saved one doesn't (the live slot would then
                        // name a binding that isn't there, or append beside an old one: R2 of
                        // m5-a11y).
                        let family = request.family.unwrap_or_else(|| binding.family());
                        let saved = mine.set_binding(&request.action, slot_for(&mine, &request.action, family), binding.clone()).map(|_| mine.bindings_json());
                        self.shell.rev += 1;
                        self.shell.settings.bindings = Some(live);
                        if self.shell.human || self.shell.store.agents_save {
                            match saved {
                                Ok(b) => {
                                    self.shell.saved.bindings = Some(b);
                                    self.shell.dirty = true;
                                }
                                // Bound for now, but not kept: say so instead of "bound" alone.
                                Err(e) => self.say(ToastKind::Warn, Said::new("toast.settings_problem").text("why", format!("the binding isn't saved: {e}"))),
                            }
                        }
                        self.apply_settings();
                    }
                    Err(e) => self.shell.rebind_status = Some(Said::new("rebind.cant_bind").text("glyph", glyph).text("why", e.to_string())),
                }
            }
        }
    }

    /// What the actions named `names` are called in the player's language, in a row ("Pause
    /// menu, Zoom"): for the controls screen's clashes.
    fn action_names(&self, names: &[String]) -> String {
        names.iter().map(|a| self.l10n.text(&Said::new(format!("action.{a}")))).collect::<Vec<_>>().join(", ")
    }

    // ---- subtitles and captions ----

    /// A timeline's subtitle line starts. Its text and speaker are string-table keys when they
    /// start with `@` (the player's language), else shown as written.
    pub(crate) fn shell_subtitle(&mut self, line: &specula_tempus::Line) {
        let until = self.now + line.duration.clamp(0.0, 60.0) as f32;
        self.shell.subtitle = Some((subtitle_words(&line.text, 240), line.speaker.as_ref().map(|s| subtitle_words(s, 40)), until));
    }

    /// Captions for what played since the last frame (the engine's audio log).
    fn read_captions(&mut self) {
        let Some(audio) = self.engine_audio.clone() else { return };
        let mut plays = std::mem::take(&mut self.shell.plays);
        self.shell.caption_seq = audio.played_after(self.shell.caption_seq, MAX_PLAYS_READ, &mut plays);
        if !self.shell.settings.captions {
            self.shell.plays = plays;
            return;
        }
        for p in &plays {
            let Some((_, words)) = self.shell.caption_table.iter().find(|(n, _)| **n == *p.name) else { continue };
            let side = p.position.zip(self.camera.as_ref()).and_then(|(at, cam)| direction(cam, at));
            let text = match side {
                Some(s) => Said::new("caption.sided").key("words", words.clone()).key("side", format!("caption.side.{s}")),
                None => Said::new("caption.plain").key("words", words.clone()),
            };
            let now = self.now;
            self.shell.captioned += 1;
            if let Some(c) = self.shell.captions.iter_mut().find(|c| *c.0 == *p.name && c.2 - CAPTION_S + CAPTION_MERGE_S > now) {
                // The same sound again: counted on the caption already shown (where it came from last).
                c.3 += 1;
                c.1 = text;
                c.2 = now + CAPTION_S;
                continue;
            }
            if self.shell.captions.len() >= MAX_CAPTIONS {
                self.shell.captions.pop_front();
            }
            self.shell.captions.push_back((p.name.to_string(), text, now + CAPTION_S, 1));
        }
        self.shell.plays = plays;
    }

    // ---- what the screens show ----

    /// The settings screens' words, made again only when what they show changed (the settings,
    /// the display, the save list), so a warm frame allocates nothing.
    fn shell_texts(&mut self) {
        let key = (self.shell.rev, self.hud.display(), self.hud.user_scale.to_bits(), self.slots.len(), self.slots.first().map(|x| x.slot));
        if self.shell.texts_key == Some(key) {
            return;
        }
        self.shell.texts_key = Some(key);
        let s = &self.shell.settings;
        let options = self.scale_options();
        let user = self.hud.user_scale;
        let eff = |u: f32| options.iter().find(|o| (o.user - u).abs() < 1e-3).map_or(u, |o| o.effective);
        let auto = self.auto_ui_scale();
        // Sizes are ratios the tables show as percentages ({pct:pct}); `eff` is the scale a size
        // gives on this display ({eff:.2}).
        let size = |key: &str, pct: f32, eff: f32| Said::new(key).num("pct", f64::from(pct)).num("eff", f64::from(eff));
        let mut texts: Vec<(String, Said)> = Vec::new();
        let mut put = |k: &str, v: Said| texts.push((k.to_owned(), v));
        put("ui_scale_text", match s.ui_scale {
            Some(_) => size("scale.chosen", user, eff(user)),
            None => size("scale.auto", auto, eff(auto)),
        });
        put("ui_opt_auto", size(if s.ui_scale.is_none() { "scale.option_auto_chosen" } else { "scale.option_auto" }, auto, eff(auto)));
        let mut flags: Vec<(String, bool)> = Vec::new();
        for (i, o) in options.iter().enumerate() {
            let chosen = s.ui_scale == Some(i);
            put(&format!("ui_opt_{i}"), size(if chosen { "scale.option_chosen" } else { "scale.option" }, o.user, o.effective));
            flags.push((format!("ui_opt_{i}_ok"), o.limited.is_none()));
            let note = match o.limited {
                Some("limited_by_window") => "scale.note.limited_by_window",
                Some(_) => "scale.note.smallest",
                None => "scale.note.changes",
            };
            put(&format!("ui_opt_{i}_note"), Said::new(note));
        }
        put("ui_scale_line", match self.shell.handheld {
            Some(h) => size("scale.now_handheld", auto, eff(user)).text("handheld", h),
            None => size("scale.now", user, eff(user)),
        });
        put("text_size_text", match s.text_size {
            Some(i) => size("text_size.percent", TEXT_SIZES[i.min(TEXT_SIZES.len() - 1)], 1.0),
            None => Said::new("text_size.auto"),
        });
        put("vision_text", Said::new(format!("vision.{}", s.vision.name())));
        put("subtitles_text", on_off(s.subtitles));
        put("captions_text", on_off(s.captions));
        put("subtitle_size_text", Said::new(format!("subtitle_size.{}", s.subtitle_size.name())));
        put("subtitle_bg_text", on_off(s.subtitle_background));
        put("camera_drag_text", Said::new(if s.camera_drag == HoldMode::Toggle { "camera_drag.toggle" } else { "camera_drag.hold" }));
        // The title's Continue.
        let newest = self.slots.iter().find(|x| x.title.key != "saves.damaged");
        flags.push(("has_save".into(), newest.is_some()));
        if let Some(n) = newest {
            put("continue_detail", match &n.detail {
                Some(d) => Said::new("title.continue_detail").said("title", n.title.clone()).said("detail", d.clone()),
                None => n.title.clone(),
            });
        }
        flags.push(("subtitle_band".into(), s.subtitle_background));
        self.shell.texts = texts;
        self.shell.flags = flags;
    }

    /// The shell's state for the screens (settings, access, controls, title, subtitles).
    pub(crate) fn fill_shell(&mut self) {
        self.read_captions();
        self.shell_texts();
        let st = &mut self.hud.state;
        for (k, v) in &self.shell.texts {
            crate::l10n::put(st, k, v);
        }
        for (k, v) in &self.shell.flags {
            st.set_bool(k, *v);
        }
        st.set_bool("has_rebind_status", self.shell.rebind_status.is_some());
        match &self.shell.rebind_status {
            Some(r) => crate::l10n::put(st, "rebind_status", r),
            // (Always there, so the hidden label has what it binds.)
            None => {
                st.set_str("rebind_status", "");
            }
        }
        // Subtitles and captions.
        let now = self.now;
        if self.shell.subtitle.as_ref().is_some_and(|x| x.2 < now) {
            self.shell.subtitle = None;
        }
        while self.shell.captions.front().is_some_and(|c| c.2 < now) {
            self.shell.captions.pop_front();
        }
        let (subtitles, captions) = (self.shell.settings.subtitles, self.shell.settings.captions);
        let st = &mut self.hud.state;
        // (The line before this one may have had a speaker: say whether this one does.)
        st.set_bool("has_speaker", subtitles && self.shell.subtitle.as_ref().is_some_and(|x| x.1.is_some()));
        if let (true, Some((text, speaker, _))) = (subtitles, &self.shell.subtitle) {
            crate::l10n::put(st, "subtitle_text", text);
            if let Some(sp) = speaker {
                crate::l10n::put(st, "subtitle_speaker", sp);
            }
        }
        let rows = st.list_mut("captions");
        rows.clear();
        let mut any = false;
        if captions {
            for (_, text, _, count) in &self.shell.captions {
                let row = rows.push();
                if *count > 1 {
                    crate::l10n::put(row, "text", &Said::new("caption.repeated").said("caption", text.clone()).int("n", i64::from(*count)));
                } else {
                    crate::l10n::put(row, "text", text);
                }
                any = true;
            }
        }
        self.hud.state.set_bool("has_captions", any);
        // The subtitles screen shows only while it has a line or a caption (it never sits over
        // the HUD empty, and menus stay the top screen).
        let want = any || (subtitles && self.shell.subtitle.is_some());
        if self.hud.visible("subtitles") != want {
            self.hud.show("subtitles", want);
        }
        self.fill_bindings();
    }

    /// The controls list: each action's keyboard-and-mouse and gamepad inputs, and clashes.
    fn fill_bindings(&mut self) {
        if !self.hud.visible("controls") {
            return;
        }
        let input = self.controls.input();
        let style = input.pad_style();
        let map = input.map();
        // (label key, keyboard and mouse, gamepad, rebindable, the actions it clashes with)
        let rows: Vec<(String, String, String, bool, String)> = listed(map)
            .into_iter()
            .map(|a| {
                let kbm = map.glyphs(&a.name, DeviceFamily::KeyboardMouse, style).join(" / ");
                let pad = map.glyphs(&a.name, DeviceFamily::Gamepad, style).join(" / ");
                let clash = self.action_names(&clashes(map, &a.name));
                (format!("action.{}", a.name), if kbm.is_empty() { "—".into() } else { kbm }, if pad.is_empty() { "—".into() } else { pad }, a.kind == specula_input::ActionKind::Button, clash)
            })
            .collect();
        let list = self.hud.state.list_mut("bindings");
        list.clear();
        for (label, kbm, pad, rebindable, clash) in rows {
            let row = list.push();
            row.set_msg("label", &label);
            row.set_str("kbm", &kbm).set_str("pad", &pad).set_bool("rebindable", rebindable).set_bool("has_conflict", !clash.is_empty());
            if !clash.is_empty() {
                crate::l10n::put(row, "conflict", &Said::new("controls.also").text("others", clash));
            }
            // The old list's single column, for anything still reading it.
            row.set_str("keys", &kbm);
        }
    }

    /// What `play_state` says about the shell: the title, where settings are saved, a handheld.
    pub(crate) fn shell_status_json(&self) -> Json {
        let mut j = Json::obj([("title", Json::from(self.shell.title)), ("settings", Json::from(if self.shell.store.kind.is_empty() { "off" } else { self.shell.store.kind }))]);
        if let Some(h) = self.shell.handheld {
            j.set("handheld", Json::from(h));
        }
        if self.shell.settings != self.shell.saved {
            j.set("settings_unsaved", Json::Bool(true));
        }
        j
    }

    /// The settings as `play_settings` and `play_state` report them.
    pub(crate) fn settings_json(&self) -> Json {
        let mut j = Json::obj([
            ("live", self.shell.settings.to_json()),
            ("saving", self.shell.store.to_json()),
            (
                "ui_scale",
                Json::obj([
                    ("user", Json::from(crate::settings::shown(self.hud.user_scale))),
                    ("auto", Json::from(self.shell.settings.ui_scale.is_none())),
                    (
                        "options",
                        Json::arr(self.scale_options().iter().map(|o| {
                            let mut e = Json::obj([("user", Json::from(crate::settings::shown(o.user))), ("effective", Json::from((f64::from(o.effective) * 1000.0).round() / 1000.0))]);
                            if let Some(l) = o.limited {
                                e.set("limited", Json::from(l));
                            }
                            e
                        })),
                    ),
                ]),
            ),
            ("handheld", self.shell.handheld.map_or(Json::Null, Json::from)),
            ("title", Json::from(self.shell.title)),
            ("rebinding", Json::from(self.shell.rebinding.is_some())),
            ("captioned", Json::from(self.shell.captioned)),
        ]);
        if self.shell.settings != self.shell.saved {
            j.set("unsaved", Json::from("an agent changed settings for this session; the player's own file keeps theirs"));
        }
        if !self.shell.problems.is_empty() {
            j.set("problems", Json::arr(self.shell.problems.iter().take(16).map(|p| Json::from(p.as_str()))));
        }
        if self.shell.refused > 0 {
            j.set("refused_agent_presses", Json::from(self.shell.refused));
        }
        if let Some(t) = self.hud.theme() {
            let d = match self.shell.settings.vision {
                Vision::Deuteranopia => Deficiency::Deutan,
                Vision::Protanopia => Deficiency::Protan,
                Vision::Tritanopia => Deficiency::Tritan,
                _ => Deficiency::None,
            };
            let c = vision::confusable(t, &MEANING_PAIRS, &[d], MIN_MEANING_DELTA_E);
            j.set(
                "colours",
                Json::obj([
                    ("for", Json::from(d.name())),
                    ("pairs_apart", Json::from(c.is_empty())),
                    ("too_close", Json::arr(c.iter().map(|x| Json::from(format!("{}/{} ΔE {:.0}", x.a, x.b, x.delta_e))))),
                ]),
            );
        }
        j
    }
}

/// Settings `play_settings {set}` may change (bindings go through `bind`).
pub const SETTABLE: [&str; 12] = ["ui_scale", "text_size", "volume", "reduced_motion", "glyphs", "autosave", "vision", "subtitles", "captions", "subtitle_size", "subtitle_background", "camera_drag"];

impl SepulcraGame {
    /// `play_settings`: read the settings, set some by name, bind inputs by name, reset the
    /// bindings. Agents' changes are the session's (the player's file keeps theirs) unless the
    /// operator named the settings folder. Errors are `(code, message)`; nothing changes on one.
    pub(crate) fn play_settings(&mut self, args: &Json) -> Result<Json, (&'static str, String)> {
        // Everything is checked before anything changes.
        let mut next: Option<Settings> = None;
        if let Some(set) = args.get("set").filter(|v| !v.is_null()) {
            let obj = set.as_object().ok_or(("bad_argument", "'set' must be an object of setting: value".to_owned()))?;
            if obj.len() > SETTABLE.len() {
                return Err(("bad_argument", format!("'set' has {} keys; at most {}", obj.len(), SETTABLE.len())));
            }
            let mut merged = self.shell.settings.to_json();
            for (k, v) in obj {
                if !SETTABLE.contains(&k.as_str()) {
                    return Err(("unknown_setting", format!("{:?} isn't a setting (one of {})", k.chars().take(64).collect::<String>(), SETTABLE.join(", "))));
                }
                merged.set(k.as_str(), v.clone());
            }
            let parsed = Settings::parse(&merged.to_string());
            if !parsed.problems.is_empty() {
                return Err(("bad_setting", parsed.problems.join("; ")));
            }
            next = Some(parsed.settings);
        }
        let binds = match args.get("bind").filter(|v| !v.is_null()) {
            None => Vec::new(),
            Some(b) => {
                let list = b.as_array().ok_or(("bad_argument", "'bind' must be an array of {action, device, input}".to_owned()))?;
                if list.len() > MAX_BINDS {
                    return Err(("bad_argument", format!("'bind' has {} entries; at most {MAX_BINDS}", list.len())));
                }
                let mut out = Vec::new();
                for (i, e) in list.iter().enumerate() {
                    let field = |k: &str| e.get(k).and_then(Json::as_str).filter(|v| (1..=64).contains(&v.len())).map(str::to_owned);
                    match (field("action"), field("device"), field("input")) {
                        (Some(a), Some(d), Some(n)) => out.push((a, d, n)),
                        _ => return Err(("bad_argument", format!("'bind[{i}]' needs action, device (kbm or pad) and input (e.g. Key.G), each 1..64 characters"))),
                    }
                }
                out
            }
        };
        let reset = args.get("reset_bindings").and_then(Json::as_bool).unwrap_or(false);
        // Bindings are tried on a copy first, so a bad one changes nothing.
        // A reset goes back to the game's own bindings where this call's changes are saved (the
        // operator's folder); for the session only, it goes back to the player's saved ones, so
        // no call takes the player's own remaps from them (R2 of m5-a11y).
        let to_players = reset && !self.shell.store.agents_save;
        let mut trial = self.controls.input().map().clone();
        if reset {
            trial = if to_players { self.players_map() } else { crate::controls::bindings().expect("the built-in bindings are valid (tested)") };
        }
        for (a, d, n) in &binds {
            let family = family_of(d).ok_or(("bad_argument", format!("device {d:?} must be kbm or pad")))?;
            let source = Source::parse(n).ok_or(("unknown_input", format!("no input named {n:?} (e.g. Key.G, Mouse.Left, Pad.North)")))?;
            let binding = Binding::source(source);
            if binding.family() != family {
                return Err(("bad_argument", format!("{n} isn't a {d} input")));
            }
            if ESSENTIAL_ACTIONS.contains(&a.as_str()) {
                return Err(("players_action", format!("{a} is how the player reaches the menu: only they rebind it (the Controls screen), so no call can take it from them")));
            }
            let act = trial.action(a).ok_or(("unknown_action", format!("no action named {a:?}")))?;
            if a.starts_with("ui_") || act.kind != specula_input::ActionKind::Button {
                return Err(("not_rebindable", format!("{a} isn't a rebindable button")));
            }
            let slot = slot_for(&trial, a, family);
            trial.set_binding(a, slot, binding).map_err(|e| ("bad_binding", e.to_string()))?;
        }
        if let Some(n) = next {
            self.change_settings(|s| {
                let (bindings, extra) = (s.bindings.take(), std::mem::take(&mut s.extra));
                *s = n.clone();
                s.bindings = bindings;
                s.extra = extra;
            });
        }
        if reset || !binds.is_empty() {
            let b = if !binds.is_empty() {
                Some(trial.bindings_json())
            } else if to_players {
                self.shell.saved.bindings.clone()
            } else {
                None
            };
            self.change_settings(|s| s.bindings = b.clone());
        }
        let mut out = self.settings_json();
        let clash: Vec<Json> = binds.iter().filter_map(|(a, _, _)| {
            let c = clashes(self.controls.input().map(), a);
            (!c.is_empty()).then(|| Json::obj([("action", Json::from(a.as_str())), ("also_used_by", Json::arr(c.into_iter().map(Json::from)))]))
        }).collect();
        if !clash.is_empty() {
            out.set("conflicts", Json::Arr(clash));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions_say_where_a_sound_is() {
        let cam = Camera::new(Vec3::new(0.0, 10.0, 10.0), Vec3::ZERO, 45.0);
        assert_eq!(direction(&cam, [-20.0, 0.0, 0.0]), Some("left"));
        assert_eq!(direction(&cam, [20.0, 0.0, 0.0]), Some("right"));
        assert_eq!(direction(&cam, [0.0, 0.0, 40.0]), Some("behind"));
        assert_eq!(direction(&cam, [0.0, 0.0, -20.0]), None, "ahead");
        let t = caption_table(CAPTIONS).unwrap();
        assert!(t.iter().any(|(n, _)| n == "boss_call"));
        assert!(caption_table("{}").is_err() && caption_table("[").is_err());
    }
}

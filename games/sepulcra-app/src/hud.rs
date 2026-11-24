//! The heads-up display: `specula-ui` screens (`games/sepulcra/assets/ui/*.json`, data agents
//! edit) laid out every window frame and drawn on the GPU overlay through the engine
//! ([`specula::game::Overlay`]).
//!
//! The game fills a [`StateMap`] each frame (wave, dust, the selected stone, threats...: see
//! [`HudInfo`]); the screens bind to it. Input reaches the UI first, through the same
//! [`Input`] as the controls, and what it takes ([`specula_ui::Consumed`]) is kept away from the
//! game (a click on a button never also places a stone). Everything the HUD shows can also be read
//! as data (`play_ui`: the widget tree) and pressed by agents by widget id, so a HUD is never only
//! for people (F1) and people are never only on a mouse (F5, F6: every button has a pad or touch
//! path).
//!
//! Without a system font (a bare CI box) the HUD is off and says why ([`Hud::problem`]); the game
//! plays the same.

use crate::l10n::{put, Said};
use crate::looks::palette;
use sepulcra::content::{Attack, Targets};
use sepulcra::{Content, Outcome};
use specula::game::Overlay;
use specula::sight::Camera;
use specula_core::math::{project_to_screen, Vec3};
use specula_core::Json;
use specula_input::{ActionMap, Input};
use specula_text::Bundled;
use specula_ui::gpu::GpuOverlay;
use specula_ui::{Display, FontSet, StateMap, Strings, Theme, ToastKind, Ui, UiEvents, Viewport};
use std::path::Path;
use std::sync::Arc;

/// The screens the game uses, with built-in copies for when the assets folder lacks them.
const SCREENS: [(&str, &str); 11] = [
    ("hud", include_str!("../../sepulcra/assets/ui/hud.json")),
    ("radial", include_str!("../../sepulcra/assets/ui/radial.json")),
    ("pause", include_str!("../../sepulcra/assets/ui/pause.json")),
    ("victory", include_str!("../../sepulcra/assets/ui/victory.json")),
    ("defeat", include_str!("../../sepulcra/assets/ui/defeat.json")),
    ("settings", include_str!("../../sepulcra/assets/ui/settings.json")),
    ("controls", include_str!("../../sepulcra/assets/ui/controls.json")),
    ("saves", include_str!("../../sepulcra/assets/ui/saves.json")),
    // Shown when a windowed game starts (the shell: crate::game's shell module).
    ("title", include_str!("../../sepulcra/assets/ui/title.json")),
    ("access", include_str!("../../sepulcra/assets/ui/access.json")),
    ("subtitles", include_str!("../../sepulcra/assets/ui/subtitles.json")),
];
const THEME: &str = include_str!("../../sepulcra/assets/ui/theme.json");
/// Colour themes for colour-vision differences (the Colours setting), beside `ui/` in the assets.
const VISION: &str = include_str!("../../sepulcra/assets/vision.json");
const CONFIRM: &str = include_str!("../../sepulcra/assets/ui/confirm.json");

/// Most widgets `play_ui` lists.
pub const MAX_TREE_WIDGETS: usize = 200;

/// Interface sizes the settings cycle through.
pub const UI_SCALES: [f32; 6] = [0.8, 0.9, 1.0, 1.15, 1.3, 1.5];

/// The UI toolkit instance, its GPU overlay and the bound state.
pub struct Hud {
    ui: Option<Ui>,
    overlay: GpuOverlay,
    pub state: StateMap,
    /// An input that never changes: captures lay the UI out with it (no hover, no presses).
    idle: Input,
    /// Why the HUD is off, if it is.
    pub problem: Option<String>,
    /// The size the UI was last laid out for (window frames).
    laid_out: Option<(u32, u32)>,
    /// The player's interface size (settings).
    pub user_scale: f32,
    /// Recent messages with their presentation time: a capture shows those still fresh (the
    /// UI ages toasts by window frames, which a headless engine never has).
    recent: std::collections::VecDeque<(f32, String, ToastKind)>,
    /// A second UI for captures (loaded at the first one), so a capture never touches what the
    /// player sees: their toasts, hover, focus and layout stay the window's.
    capture: Option<(Ui, GpuOverlay)>,
    /// Where the screens were loaded from (the capture UI loads the same).
    dir: Option<std::path::PathBuf>,
    /// The display the engine says the window (or, headless, the agents' virtual one) is: its
    /// OS scale and safe area apply to captures too, at their own size.
    display: Display,
    /// The player's language ([`crate::l10n`]): the window's UI and the capture UI show it.
    strings: Option<Arc<Strings>>,
    /// How the player's settings change the theme (text size, colours, motion), applied to the
    /// window's UI and the capture UI alike.
    look: Look,
    /// The theme and the colour themes as read and parsed, kept until a hot reload: a look
    /// change (every settings change an agent or the player makes) composes from these instead
    /// of reading both files on the engine thread each time (review R1 of m5-a11y).
    theme_src: Option<(Json, Json)>,
}

/// What the player's settings change in the theme ([`Hud::set_look`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// Multiplies every text role's size.
    pub text_size: f32,
    /// Multiplies the subtitle and caption roles' sizes again.
    pub subtitle_size: f32,
    /// A colour theme's name in `vision.json` (`standard`: none).
    pub vision: &'static str,
    /// No pulsing (threat arrows).
    pub reduced_motion: bool,
}

impl Default for Look {
    fn default() -> Look {
        Look { text_size: 1.0, subtitle_size: 1.0, vision: "standard", reduced_motion: false }
    }
}

/// The text roles subtitles and captions use (sized again by the subtitle size).
const SUBTITLE_ROLES: [&str; 3] = ["subtitle_line", "speaker", "caption"];

/// The theme's JSON with `look` applied: role sizes, a colour theme's colours and focus width,
/// no pulsing with reduced motion. `vision` is `vision.json`.
pub fn compose_theme(base: &Json, vision: &Json, look: &Look) -> Json {
    let mut t = base.clone();
    if let Some(Json::Obj(roles)) = t.get("roles").cloned() {
        let roles = roles
            .into_iter()
            .map(|(name, mut r)| {
                let k = look.text_size * if SUBTITLE_ROLES.contains(&name.as_str()) { look.subtitle_size } else { 1.0 };
                if let Some(size) = r.get("size").and_then(Json::as_f64) {
                    r.set("size", Json::from((size * f64::from(k) * 100.0).round() / 100.0));
                }
                (name, r)
            })
            .collect();
        t.set("roles", Json::Obj(roles));
    }
    if let Some(patch) = vision.get("themes").and_then(|th| th.get(look.vision)) {
        if let (Some(Json::Obj(colors)), Some(Json::Obj(over))) = (t.get("colors").cloned(), patch.get("colors")) {
            let colors = colors.into_iter().map(|(k, v)| (k.clone(), over.iter().find(|(o, _)| *o == k).map_or(v, |(_, c)| c.clone()))).collect();
            t.set("colors", Json::Obj(colors));
        }
        if let (Some(w), Some(mut focus)) = (patch.get("focus_width").and_then(Json::as_f64), t.get("focus").cloned()) {
            focus.set("width", Json::from(w));
            t.set("focus", focus);
        }
    }
    if (look.text_size - 1.0).abs() > 1e-4 {
        t.set("text_scale", Json::from(f64::from(look.text_size)));
    }
    if look.reduced_motion {
        if let Some(mut threat) = t.get("threat").cloned() {
            threat.set("pulse_hz", Json::from(0.0));
            t.set("threat", threat);
        }
    }
    t
}

/// Screens the game shows and hides (the capture UI mirrors their visibility).
const SHOWN: [&str; 12] = ["hud", "radial", "pause", "victory", "defeat", "settings", "controls", "saves", "confirm", "title", "access", "subtitles"];

/// Seconds a message stays on a capture.
const CAPTURE_TOAST_S: f32 = 4.0;
/// Most toasts a capture shows.
const CAPTURE_TOASTS: usize = 3;

/// The UI's viewport for an image of `size` on `display` (its OS scale and safe area) at the
/// player's interface size: the same as the engine's `ui_*` tools compute.
fn viewport(ui: &Ui, display: &Display, size: (u32, u32), user_scale: f32) -> Viewport {
    Display { size: [size.0.max(1), size.1.max(1)], ..*display }.viewport(ui.theme(), user_scale)
}

/// Largest interface file read (a theme or a screen is a few KiB); a larger one is passed over
/// for the built-in copy.
const MAX_UI_FILE: u64 = 512 << 10;

fn read(dir: Option<&Path>, name: &str, builtin: &'static str) -> String {
    dir.map(|d| d.join(format!("{name}.json")))
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= MAX_UI_FILE))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| builtin.to_owned())
}

impl Hud {
    /// Loads fonts, the theme and the screens (from `ui_dir` when given, else the built-in
    /// copies). `map` is the game's input map (prompts show its bindings).
    pub fn new(ui_dir: Option<&Path>, map: ActionMap) -> Hud {
        let idle = Input::new(map);
        let mut hud = Hud {
            ui: None,
            overlay: GpuOverlay::new(),
            state: StateMap::new(),
            idle,
            problem: None,
            laid_out: None,
            user_scale: 1.0,
            recent: std::collections::VecDeque::new(),
            capture: None,
            dir: ui_dir.map(Path::to_path_buf),
            display: Display::new([1280, 720]),
            strings: None,
            look: Look::default(),
            theme_src: None,
        };
        match Hud::load(ui_dir) {
            Ok(ui) => hud.ui = Some(ui),
            Err(e) => {
                eprintln!("sepulcra: no HUD: {e}");
                hud.problem = Some(e);
            }
        }
        hud
    }

    fn load(dir: Option<&Path>) -> Result<Ui, String> {
        // The bundled SIL OFL faces (STYLE.md: a clean sans for the interface, the serif only for
        // titles): the same on every machine, and shippable.
        let face = |b: Bundled| b.load().map_err(|e| format!("bundled font {}: {e}", b.name()));
        let fonts = FontSet::new().with("ui", face(Bundled::UiRegular)?).with("ui_bold", face(Bundled::UiBold)?).with("title", face(Bundled::Display)?);
        let theme = Theme::from_json_str(&read(dir, "theme", THEME)).map_err(|e| format!("ui/theme.json: {e}"))?;
        let mut ui = Ui::new(theme, fonts);
        for (name, builtin) in SCREENS.iter().chain([("confirm", CONFIRM)].iter()) {
            ui.load_screen(&read(dir, name, builtin)).map_err(|e| format!("ui/{name}.json: {e}"))?;
        }
        ui.show("hud").map_err(|e| e.to_string())?;
        Ok(ui)
    }

    /// The assets' theme and colour themes, parsed (read once; again after a hot reload).
    fn theme_sources(&mut self) -> Result<(Json, Json), String> {
        if self.theme_src.is_none() {
            let dir = self.dir.as_deref();
            let base = Json::parse(&read(dir, "theme", THEME)).map_err(|e| format!("ui/theme.json: {}", e.message))?;
            let vision = Json::parse(&read(dir.and_then(Path::parent), "vision", VISION)).map_err(|e| format!("vision.json: {}", e.message))?;
            self.theme_src = Some((base, vision));
        }
        Ok(self.theme_src.clone().expect("just filled"))
    }

    /// The theme with `look` applied, from the assets' theme and colour themes.
    fn theme_for(src: &(Json, Json), look: &Look) -> Result<Theme, String> {
        Theme::from_json(&compose_theme(&src.0, &src.1, look)).map_err(|e| format!("ui/theme.json with the player's settings: {e}"))
    }

    /// Applies the player's look (text size, colours, motion) to the window's UI and the capture
    /// UI. A theme that doesn't build keeps the one there (the error is returned and kept).
    pub fn set_look(&mut self, look: &Look) -> Result<(), String> {
        if *look == self.look {
            return Ok(());
        }
        self.look = look.clone();
        self.apply_look()
    }

    fn apply_look(&mut self) -> Result<(), String> {
        if self.ui.is_none() {
            return Ok(());
        }
        let theme = self.theme_sources().and_then(|src| Hud::theme_for(&src, &self.look));
        let theme = theme.inspect_err(|e| self.problem = Some(e.clone()))?;
        for ui in self.ui.iter_mut().chain(self.capture.as_mut().map(|(ui, _)| ui)) {
            ui.set_theme(theme.clone()).map_err(|e| e.to_string())?;
        }
        self.laid_out = None;
        Ok(())
    }

    pub fn look(&self) -> &Look {
        &self.look
    }

    /// A row action (`rebind_kbm@row`) pressed on row N of a list (its widget `bind_kbm[N]`)
    /// becomes `rebind_kbm:N`; any other action is as it is.
    pub fn row_action(&self, action: &str, widget: specula_ui::WidgetRef) -> String {
        let Some(base) = action.strip_suffix("@row") else { return action.to_owned() };
        let row = self.ui.as_ref().and_then(|ui| ui.widget_id(widget)).and_then(|id| id.rsplit_once('[')).and_then(|(_, n)| n.strip_suffix(']')).and_then(|n| n.parse::<usize>().ok());
        match row {
            Some(n) => format!("{base}:{n}"),
            None => base.to_owned(),
        }
    }

    /// Hot reload: the theme and screens whose files changed (`names`: file stems under `ui/`,
    /// `theme` for the theme), in the window's UI and the capture UI. Screens keep their
    /// visibility. A file that doesn't load keeps what was there; its error is returned.
    pub fn reload(&mut self, names: &[String]) -> Vec<String> {
        let mut errors = Vec::new();
        let Some(dir) = self.dir.clone() else { return errors };
        // The theme's files changed: read them again (once, for both interfaces).
        let retheme = names.iter().any(|n| n == "theme" || n == "vision");
        if retheme {
            self.theme_src = None;
        }
        let themed = retheme.then(|| self.theme_sources().and_then(|src| Hud::theme_for(&src, &self.look)));
        let uis = self.ui.iter_mut().chain(self.capture.as_mut().map(|(ui, _)| ui));
        for ui in uis {
            if let Some(themed) = &themed {
                let r = themed.clone().and_then(|t| ui.set_theme(t).map_err(|e| e.to_string()));
                if let Err(e) = r {
                    errors.push(format!("ui/theme.json: {e}"));
                }
            }
            for n in names.iter().filter(|n| *n != "theme" && *n != "vision") {
                let r = std::fs::read_to_string(dir.join(format!("{n}.json"))).map_err(|e| e.to_string()).and_then(|t| ui.load_screen(&t).map(|_| ()).map_err(|e| e.to_string()));
                if let Err(e) = r {
                    errors.push(format!("ui/{n}.json: {e}"));
                }
            }
        }
        // Lay out again on the next frame.
        self.laid_out = None;
        errors.sort();
        errors.dedup();
        if let Some(e) = errors.first() {
            self.problem = Some(e.clone());
        }
        errors
    }

    /// A HUD that draws nothing (before the game starts).
    pub fn off(why: &str) -> Hud {
        Hud {
            ui: None,
            overlay: GpuOverlay::new(),
            state: StateMap::new(),
            idle: Input::new(ActionMap::new()),
            problem: Some(why.to_owned()),
            laid_out: None,
            user_scale: 1.0,
            recent: std::collections::VecDeque::new(),
            capture: None,
            dir: None,
            display: Display::new([1280, 720]),
            strings: None,
            look: Look::default(),
            theme_src: None,
        }
    }

    /// Shows `strings`' language from the next frame (the window's UI and the capture UI).
    pub fn set_strings(&mut self, strings: Arc<Strings>) {
        if let Some(ui) = &mut self.ui {
            ui.set_strings(Some(Arc::clone(&strings)));
        }
        if let Some((ui, _)) = &mut self.capture {
            ui.set_strings(Some(Arc::clone(&strings)));
        }
        self.strings = Some(strings);
        self.laid_out = None;
    }

    pub fn enabled(&self) -> bool {
        self.ui.is_some()
    }

    /// One window frame: input in, layout, events out (what the player pressed, what the UI took).
    /// `out` is overwritten (its storage reused: no allocation once warm).
    pub fn frame(&mut self, input: &Input, size: (u32, u32), out: &mut UiEvents) {
        // Its own zone (perf_zones: .../game_frame/hud_frame), for the HUD's CPU budget.
        let _z = specula_core::profile::zone("hud_frame");
        // Layouts without input (captures, presses by id) show the glyphs the player sees.
        self.idle.set_last_device(input.last_device());
        self.idle.set_pad_style(input.pad_style());
        out.events.clear();
        out.consumed = Default::default();
        let Some(ui) = self.ui.as_mut() else { return };
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        let view = viewport(ui, &self.display, size, self.user_scale);
        self.laid_out = Some(size);
        out.clone_from(ui.frame(input, &self.state, &view).events);
    }

    /// The overlay quads for one image: the window (laid out by [`Hud::frame`] this frame) or a
    /// capture (laid out here for its size, with no input).
    /// `now`: presentation seconds (which messages a capture still shows).
    pub fn overlay(&mut self, out: &mut Overlay, now: f32) {
        let _z = specula_core::profile::zone("hud_overlay");
        if out.capture {
            self.capture_overlay(out, now);
            return;
        }
        let Some(ui) = self.ui.as_mut() else { return };
        if self.laid_out != Some(out.size) {
            let view = viewport(ui, &self.display, out.size, self.user_scale);
            ui.frame(&self.idle, &self.state, &view);
            // The window lays out again for itself on its next frame.
            self.laid_out = None;
        }
        if let Err(e) = self.overlay.sync_atlas(out.textures, ui.atlas_mut()) {
            self.problem = Some(format!("glyph atlas: {e}"));
            return;
        }
        match self.overlay.build(ui) {
            Ok(quads) => out.quads.extend_from_slice(quads),
            Err(e) => self.problem = Some(format!("overlay: {e}")),
        }
    }

    /// A capture's interface, from the capture UI: the window's screens, the messages still
    /// fresh at `now`, laid out for the capture's size with no input.
    fn capture_overlay(&mut self, out: &mut Overlay, now: f32) {
        let themed = (self.capture.is_none() && self.ui.is_some() && self.look != Look::default()).then(|| self.theme_sources().and_then(|src| Hud::theme_for(&src, &self.look)));
        let Some(win) = self.ui.as_ref() else { return };
        if self.capture.is_none() {
            let loaded = Hud::load(self.dir.as_deref()).and_then(|mut ui| {
                if let Some(themed) = themed {
                    ui.set_theme(themed?).map_err(|e| e.to_string())?;
                }
                ui.set_strings(self.strings.clone());
                Ok(ui)
            });
            match loaded {
                Ok(ui) => self.capture = Some((ui, GpuOverlay::new())),
                Err(e) => {
                    self.problem = Some(format!("capture HUD: {e}"));
                    return;
                }
            }
        }
        let Some((ui, overlay)) = self.capture.as_mut() else { return };
        for name in SHOWN {
            let open = win.is_visible(name);
            if ui.is_visible(name) != open {
                let _ = if open { ui.show(name) } else { ui.hide(name) };
            }
        }
        ui.clear_toasts();
        let fresh = self.recent.iter().filter(|(t, _, _)| now - *t < CAPTURE_TOAST_S && *t <= now).count();
        for (_, text, kind) in self.recent.iter().filter(|(t, _, _)| now - *t < CAPTURE_TOAST_S && *t <= now).skip(fresh.saturating_sub(CAPTURE_TOASTS)) {
            ui.toast(text, *kind);
        }
        let view = viewport(ui, &self.display, out.size, self.user_scale);
        // The player's focus and list scroll, not the screens' defaults (ui_frame outlines the
        // capture's: review R1 of m5-uitools, finding 2).
        ui.copy_interaction_from(win, view.scale);
        ui.frame(&self.idle, &self.state, &view);
        if let Err(e) = overlay.sync_atlas(out.textures, ui.atlas_mut()) {
            self.problem = Some(format!("capture glyph atlas: {e}"));
            return;
        }
        match overlay.build(ui) {
            Ok(quads) => out.quads.extend_from_slice(quads),
            Err(e) => self.problem = Some(format!("capture overlay: {e}")),
        }
    }

    /// Says `text` at presentation time `now`.
    pub fn toast(&mut self, text: &str, kind: ToastKind, now: f32) {
        if let Some(ui) = &mut self.ui {
            ui.toast(text, kind);
        }
        if self.recent.len() == 8 {
            self.recent.pop_front();
        }
        self.recent.push_back((now, text.to_owned(), kind));
    }

    pub fn show(&mut self, screen: &str, visible: bool) {
        if let Some(ui) = &mut self.ui {
            let r = if visible { ui.show(screen) } else { ui.hide(screen) };
            if let Err(e) = r {
                eprintln!("sepulcra: ui {screen}: {e}");
            }
        }
    }

    pub fn visible(&self, screen: &str) -> bool {
        self.ui.as_ref().is_some_and(|ui| ui.is_visible(screen))
    }

    /// Visible screens, bottom to top.
    pub fn screens(&self) -> Vec<String> {
        self.ui.as_ref().map(|ui| ui.visible_screens().map(str::to_owned).collect()).unwrap_or_default()
    }

    /// Presses a widget by id (`play_ui`), as a click or a pad's accept would. Its action comes
    /// out of the next [`Hud::frame`] (agents: of [`Hud::press`]).
    pub fn press(&mut self, id: &str) -> Result<Option<String>, String> {
        let ui = self.ui.as_mut().ok_or("the HUD is off")?;
        // Lay out the current state first, so what's visible and enabled is what a player sees now.
        let size = self.laid_out.unwrap_or((self.display.size[0], self.display.size[1]));
        let view = viewport(ui, &self.display, size, self.user_scale);
        ui.frame(&self.idle, &self.state, &view);
        let screens: Vec<String> = ui.visible_screens().map(str::to_owned).collect();
        // Topmost screen first: a modal's buttons win over the HUD's.
        let w = screens.iter().rev().find_map(|s| ui.widget(s, id)).ok_or_else(|| format!("no widget '{id}' on the visible screens ({})", screens.join(", ")))?;
        if ui.rect(w).is_none() {
            return Err(format!("'{id}' isn't showing (a player couldn't press it now)"));
        }
        ui.activate(w).map_err(|e| e.to_string())?;
        // Activation sends its action through the next frame's events: collect it now.
        let events = ui.frame(&self.idle, &self.state, &view).events.clone();
        self.laid_out = None;
        if events.events.iter().any(|e| matches!(e, specula_ui::UiEvent::Refused { .. })) {
            return Err(format!("'{id}' is disabled (its condition doesn't hold, e.g. not enough dust)"));
        }
        let action = events.events.iter().find_map(|e| match e {
            specula_ui::UiEvent::Action { action, widget } => Some(self.row_action(action, *widget)),
            _ => None,
        });
        match action {
            Some(a) => Ok(Some(a)),
            None => Err(format!("pressing '{id}' sent no action")),
        }
    }

    /// The display the engine says the interface is on ([`specula::game::Game::display`]).
    pub fn set_display(&mut self, display: &Display) {
        if self.display != *display {
            self.display = *display;
            self.laid_out = None;
        }
    }

    pub fn display(&self) -> Display {
        self.display
    }

    /// The live theme (the player's look applied), while the HUD is on.
    pub fn theme(&self) -> Option<&Theme> {
        self.ui.as_ref().map(|ui| ui.theme())
    }

    /// The live UI (`capture`: the one the last capture drew) and its state, for the engine's
    /// `ui_*` tools. `None` while the HUD is off.
    pub fn ui_parts(&mut self, capture: bool) -> Option<(&mut Ui, &StateMap)> {
        let ui = if capture { self.capture.as_mut().map(|(ui, _)| ui) } else { self.ui.as_mut() }?;
        Some((ui, &self.state))
    }

    /// The visible widgets that matter to a player as data (those with an id or text: id, type,
    /// rect in whole pixels, text, enabled, focused), at most `limit`.
    pub fn tree(&self, limit: usize) -> Json {
        let Some(ui) = &self.ui else {
            return Json::obj([("off", Json::from(self.problem.as_deref().unwrap_or("no HUD")))]);
        };
        let tree = ui.tree();
        // Keep the reply bounded: cut each screen's widget list to what's left of `limit`.
        let mut left = limit.min(MAX_TREE_WIDGETS);
        let mut cut = 0usize;
        let mut screens = Vec::new();
        for s in tree.get("screens").and_then(Json::as_array).unwrap_or(&[]) {
            let mut s2 = s.clone();
            if let Some(ws) = s.get("widgets").and_then(Json::as_array) {
                let useful: Vec<Json> = ws.iter().filter(|w| w.get("id").is_some() || w.get("text").is_some()).map(compact_widget).collect();
                let keep = useful.len().min(left);
                cut += useful.len() - keep;
                left -= keep;
                s2.set("widgets", Json::arr(useful.into_iter().take(keep)));
            }
            screens.push(s2);
        }
        let mut out = Json::obj([("screens", Json::Arr(screens))]);
        if cut > 0 {
            out.set("truncated", Json::from(cut));
        }
        out
    }
}

/// A widget for `play_ui`: its rect rounded to whole pixels, without the tree path.
fn compact_widget(w: &Json) -> Json {
    let mut out = Vec::new();
    for (k, v) in w.as_object().unwrap_or(&[]) {
        match k.as_str() {
            "path" => {}
            "rect" => out.push((k.clone(), Json::arr(v.as_array().unwrap_or(&[]).iter().map(|n| Json::from(n.as_f64().unwrap_or(0.0).round()))))),
            _ => out.push((k.clone(), v.clone())),
        }
    }
    Json::obj(out)
}

/// What the HUD shows, gathered by the game.
pub struct HudInfo<'a> {
    pub content: &'a Content,
    pub wave: u16,
    pub waves: u16,
    pub next_wave_s: Option<u32>,
    pub early_bonus: i32,
    pub dust: i32,
    pub tombstones: i32,
    pub paused: bool,
    pub speed: u32,
    /// Stone index, or `None` for the wall.
    pub selected: Option<u8>,
    pub hover: Option<&'a Said>,
    /// The selected cell's stone (kind index, tier) or wall (hp, max hp), with what upgrading
    /// costs and selling gives back.
    pub inspect: Option<Inspect>,
    pub kills: u32,
    pub run_time_s: f64,
    pub outcome: Outcome,
    pub volume: f32,
    pub reduced_motion: bool,
    /// The button-prompt style's table key (`glyphs.auto`).
    pub glyph_style: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Inspect {
    Stone { kind: u8, tier: u8, upgrade: Option<i32>, refund: i32 },
    Wall { hp: i32, max_hp: i32, refund: i32 },
}

/// The HUD's name for a stone kind (icons, build buttons, the radial menu).
pub fn icon_of(stone_id: &str) -> &'static str {
    match stone_id {
        "rune_pillar" => "pillar",
        "brazier" => "brazier",
        "sky_obelisk" => "obelisk",
        _ => "warding",
    }
}

fn secs(ticks: u32) -> f64 {
    f64::from(ticks) / f64::from(sepulcra::TICK_HZ)
}

/// A stone's name at a tier ("Warding stone II"), in the player's language.
pub fn stone_name(content: &Content, kind: u8, tier: u8) -> Said {
    let id = content.stones.get(usize::from(kind)).map_or("?", |d| d.id.as_str());
    Said::new("stone.tier_name").key("name", format!("stone.{id}.name")).key("tier", format!("tier.{}", tier.min(2) + 1))
}

/// What a stone does at a tier (its numbers from the content).
pub fn stone_detail(content: &Content, kind: u8, tier: u8) -> Said {
    let def = &content.stones[usize::from(kind)];
    let Some(t) = def.tiers.get(usize::from(tier)) else { return Said::new("stone.fully_carved") };
    let range = t.range as f64 / 1000.0;
    let dmg = t.damage as f64 / 1000.0;
    let s = match def.attack {
        Attack::Bolt => Said::new("stone.detail.bolt"),
        Attack::Lob => Said::new("stone.detail.lob").num("splash", t.splash as f64 / 1000.0),
        Attack::Pulse => Said::new("stone.detail.pulse").int("slow", t.slow / 10).num("slow_time", secs(t.slow_ticks)),
        // PLACEHOLDER: the elemental stones (proving ground only) are said as the nearest shipped
        // attack until their own strings are written in every language.
        Attack::Strike | Attack::Heave => Said::new("stone.detail.bolt"),
        Attack::Gust => Said::new("stone.detail.pulse").int("slow", 0).num("slow_time", 0.0),
    };
    s.num("damage", dmg).num("cooldown", secs(t.cooldown)).num("range", range)
}

fn role(content: &Content, kind: u8) -> Said {
    let def = &content.stones[usize::from(kind)];
    Said::new(match (def.attack, def.targets) {
        (_, Targets::Air) => "stone.role.anti_air",
        (Attack::Pulse, _) => "stone.role.area_slow",
        (Attack::Lob, _) => "stone.role.splash",
        _ => "stone.role.single",
    })
}

/// The build bar's state keys per icon: selected, cost, affordable.
const BAR_KEYS: [(&str, &str, &str, &str); 4] = [
    ("warding", "sel_warding", "cost_warding", "can_warding"),
    ("pillar", "sel_pillar", "cost_pillar", "can_pillar"),
    ("brazier", "sel_brazier", "cost_brazier", "can_brazier"),
    ("obelisk", "sel_obelisk", "cost_obelisk", "can_obelisk"),
];

/// The HUD's messages that change only with the content, made once and reused: a frame's fill
/// puts them in the state, whose storage is reused (F3: no allocation once warm).
#[derive(Default)]
pub struct Texts {
    content: Option<u64>,
    /// Per stone kind and tier: its name, what that tier does, and what the next adds.
    names: Vec<[Said; 3]>,
    details: Vec<[Said; 3]>,
    upgrades: Vec<[Said; 3]>,
    roles: Vec<Said>,
}

impl Texts {
    fn ensure(&mut self, content: &Content) {
        if self.content == Some(content.hash) {
            return;
        }
        self.content = Some(content.hash);
        let kinds = 0..content.stones.len() as u8;
        self.names = kinds.clone().map(|k| [0, 1, 2].map(|t| stone_name(content, k, t))).collect();
        self.details = kinds.clone().map(|k| [0, 1, 2].map(|t| stone_detail(content, k, t))).collect();
        self.upgrades = kinds
            .clone()
            .map(|k| [0u8, 1, 2].map(|t| if t < 2 { Said::new("stone.upgrade").key("tier", format!("tier.{}", t + 2)).said("detail", stone_detail(content, k, t + 1)) } else { Said::new("stone.fully_carved") }))
            .collect();
        self.roles = kinds.map(|k| role(content, k)).collect();
    }
}

/// Fills `s` from `info` (everything the screens bind to; see their `notes`). Words are messages
/// ([`put`]): the HUD shows them in the player's language. `texts` keeps the messages between
/// frames.
pub fn fill(s: &mut StateMap, info: &HudInfo, texts: &mut Texts) {
    let content = info.content;
    texts.ensure(content);
    s.reset();
    s.set_int("wave", i64::from(info.wave)).set_int("waves", i64::from(info.waves));
    if let Some(n) = info.next_wave_s {
        s.set_num("next_wave_s", f64::from(n)).set_int("early_bonus", i64::from(info.early_bonus));
    }
    s.set_int("dust", i64::from(info.dust)).set_int("tombstones", i64::from(info.tombstones));
    s.set_int("tombstones_max", i64::from(content.rules.tombstones));
    s.set_bool("paused", info.paused).set_int("speed", i64::from(info.speed));
    s.set_bool("running", info.outcome == Outcome::Running);
    // The selection and the build bar.
    for (i, def) in content.stones.iter().enumerate() {
        let icon = icon_of(&def.id);
        let Some(&(_, sel, cost_key, can)) = BAR_KEYS.iter().find(|k| k.0 == icon) else { continue };
        let cost = def.tiers[0].cost;
        s.set_bool(sel, info.selected == Some(i as u8));
        s.set_int(cost_key, i64::from(cost));
        s.set_bool(can, info.dust >= cost);
    }
    let wall_cost = content.rules.wall.cost;
    s.set_bool("sel_wall", info.selected.is_none()).set_int("cost_wall", i64::from(wall_cost)).set_bool("can_wall", info.dust >= wall_cost);
    match info.selected.filter(|&i| usize::from(i) < content.stones.len()) {
        Some(i) => {
            let def = &content.stones[usize::from(i)];
            let k = usize::from(i);
            s.set_str("selected", icon_of(&def.id));
            put(s, "selected_name", &texts.names[k][0]);
            put(s, "selected_role", &texts.roles[k]);
            put(s, "selected_detail", &texts.details[k][0]);
            s.set_int("selected_cost", i64::from(def.tiers[0].cost)).set_bool("affordable", info.dust >= def.tiers[0].cost);
        }
        None => {
            s.set_str("selected", "wall");
            s.set_msg("selected_name", "wall.name");
            s.set_msg("selected_role", "wall.role");
            s.set_msg("selected_detail", "wall.detail");
            s.set_int("selected_cost", i64::from(wall_cost)).set_bool("affordable", info.dust >= wall_cost);
        }
    }
    if let Some(h) = info.hover {
        put(s, "hover", h);
    }
    match info.inspect {
        Some(Inspect::Stone { kind, tier, upgrade, refund }) => {
            let def = &content.stones[usize::from(kind)];
            let (k, t) = (usize::from(kind), usize::from(tier.min(2)));
            put(s, "inspect", &texts.names[k][t]);
            s.set_str("inspect_icon", icon_of(&def.id));
            put(s, "inspect_detail", &texts.details[k][t]);
            if let Some(cost) = upgrade {
                s.set_int("upgrade_cost", i64::from(cost)).set_bool("can_upgrade", info.dust >= cost);
            }
            put(s, "upgrade_detail", &texts.upgrades[k][if upgrade.is_some() { t.min(1) } else { 2 }]);
            s.set_int("refund", i64::from(refund)).set_bool("inspect_stone", true);
        }
        Some(Inspect::Wall { hp, max_hp, refund }) => {
            s.set_msg("inspect", "wall.name");
            s.set_str("inspect_icon", "wall");
            s.set_msg("inspect_detail", "wall.inspect").set_int("hp", i64::from(hp / 1000)).set_int("max_hp", i64::from(max_hp / 1000));
            s.set_int("refund", i64::from(refund)).set_bool("inspect_wall", true);
        }
        None => {}
    }
    s.set_int("kills", i64::from(info.kills)).set_num("run_time_s", info.run_time_s);
    // Settings.
    s.set_num("volume", f64::from(info.volume));
    s.set_msg("reduced_motion_text", if info.reduced_motion { "common.on" } else { "common.off" });
    s.set_msg("glyph_style", info.glyph_style);
    s.set_msg("language", "language.name");
}

/// Sets `ui_scale` (the settings screen shows it).
pub fn fill_scale(s: &mut StateMap, scale: f32) {
    s.set_num("ui_scale", f64::from(scale));
}

/// Where threats are: one row per face (or the air) with enemies the camera can't see, for the
/// screen-edge arrows (STYLE.md: grave green, pointing at the face under attack). `groups` are
/// `(label key, centre of its enemies, count)`.
pub fn fill_threats(s: &mut StateMap, camera: &Camera, size: (u32, u32), groups: &[(&str, Vec3, usize)]) {
    let rows = s.list_mut("threats");
    rows.clear();
    let (w, h) = (size.0.max(1), size.1.max(1));
    let vp = camera.view_proj(w, h);
    let margin = 0.04 * w.min(h) as f32;
    for &(label, at, count) in groups {
        if count == 0 {
            continue;
        }
        // Behind the camera: mirror the direction so the arrow points the way to turn.
        let (x, y, behind) = match project_to_screen(&vp, at, w, h) {
            Some((x, y, _)) => (x, y, false),
            None => {
                let back = camera.eye + (camera.eye - at);
                match project_to_screen(&vp, back, w, h) {
                    Some((x, y, _)) => (w as f32 - x, y, true),
                    None => (w as f32 / 2.0, h as f32, true),
                }
            }
        };
        let inside = !behind && x > margin && y > margin && x < w as f32 - margin && y < h as f32 - margin;
        if inside {
            continue;
        }
        let row = rows.push();
        row.set_num("x", f64::from(x)).set_num("y", f64::from(y)).set_bool("behind", behind);
        row.set_msg("label", label).set_int("count", count as i64);
    }
}

/// The HUD palette check (STYLE.md): threats use the enemy's green, never the player's cyan.
pub const THREAT_COLOR: [f32; 3] = palette::GRAVE_GLOW;

#[cfg(test)]
mod tests {
    use super::*;

    /// R1 finding 15 of m5-a11y: a look change (every settings change) composes the theme from
    /// what was read once, and reads the files again only on a hot reload.
    #[test]
    fn a_look_change_reads_no_theme_file_until_a_hot_reload() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sepulcra/assets");
        let dir = std::env::temp_dir().join(format!("sepulcra-hud-theme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ui")).unwrap();
        for e in std::fs::read_dir(assets.join("ui")).unwrap() {
            let p = e.unwrap().path();
            std::fs::copy(&p, dir.join("ui").join(p.file_name().unwrap())).unwrap();
        }
        std::fs::copy(assets.join("vision.json"), dir.join("vision.json")).unwrap();
        let mut hud = Hud::new(Some(&dir.join("ui")), crate::controls::bindings().unwrap());
        assert!(hud.theme().is_some(), "{:?}", hud.problem);
        let look = |text_size: f32| Look { text_size, ..Look::default() };
        hud.set_look(&look(1.15)).unwrap();
        // The files become unreadable as themes: a look change doesn't notice, because it
        // doesn't read them.
        std::fs::write(dir.join("ui").join("theme.json"), "not a theme").unwrap();
        std::fs::write(dir.join("vision.json"), "not json either").unwrap();
        hud.set_look(&look(1.3)).expect("composed from what was read once");
        // A hot reload reads them again, and says what is wrong with them.
        let errors = hud.reload(&["theme".to_owned()]);
        assert!(errors.iter().any(|e| e.contains("theme.json")), "{errors:?}");
        // An oversized file is passed over for the built-in copy.
        std::fs::write(dir.join("ui").join("theme.json"), vec![b' '; (MAX_UI_FILE + 1) as usize]).unwrap();
        std::fs::copy(assets.join("vision.json"), dir.join("vision.json")).unwrap();
        assert!(hud.reload(&["theme".to_owned()]).is_empty(), "the built-in theme stands in");
        let _ = std::fs::remove_dir_all(&dir);
    }
    use sepulcra::Content;

    fn info(content: &Content) -> HudInfo<'_> {
        HudInfo {
            content,
            wave: 3,
            waves: 10,
            next_wave_s: Some(12),
            early_bonus: 6,
            dust: 55,
            tombstones: 9,
            paused: false,
            speed: 1,
            selected: Some(0),
            hover: None,
            inspect: Some(Inspect::Stone { kind: 2, tier: 1, upgrade: Some(110), refund: 112 }),
            kills: 40,
            run_time_s: 200.0,
            outcome: Outcome::Running,
            volume: 0.75,
            reduced_motion: false,
            glyph_style: "glyphs.auto",
        }
    }

    #[test]
    fn the_state_has_what_the_screens_bind_to() {
        let content = Content::builtin();
        let mut s = StateMap::new();
        let mut texts = Texts::default();
        fill(&mut s, &info(&content), &mut texts);
        for key in ["wave", "waves", "next_wave_s", "dust", "tombstones", "tombstones_max", "selected", "selected_cost", "affordable", "selected_name", "language"] {
            assert!(s.get(key).is_some(), "{key} missing");
        }
        assert!(matches!(s.get("sel_warding"), Some(specula_ui::Value::Bool(true))));
        assert!(matches!(s.get("can_brazier"), Some(specula_ui::Value::Bool(false))), "55 dust can't buy an 80-dust brazier");
        // Words are messages in the player's language: the brazier at tier II.
        let l10n = crate::l10n::L10n::load(None, None);
        let mut shown = String::new();
        specula_ui::state::Template::parse("{inspect}", "t").unwrap().render(&specula_ui::state::Scope::new(&s).with_strings(Some(&l10n.strings())), &mut shown);
        assert_eq!(shown, "Brazier cairn II");
        assert!(matches!(s.get("inspect"), Some(specula_ui::Value::Msg(m)) if m.key == "stone.tier_name"), "{:?}", s.get("inspect"));
        assert!(matches!(s.get("can_upgrade"), Some(specula_ui::Value::Bool(false))));
        // Nothing to inspect, no wave timer: the keys are gone (the panels hide).
        fill(&mut s, &HudInfo { inspect: None, next_wave_s: None, ..info(&content) }, &mut texts);
        assert!(s.get("inspect").is_none() && s.get("next_wave_s").is_none());
    }

    #[test]
    fn threats_point_at_faces_out_of_view() {
        let cam = Camera::new(Vec3::new(0.0, 30.0, 40.0), Vec3::ZERO, 45.0);
        let mut s = StateMap::new();
        let groups = [("Top", Vec3::ZERO, 3), ("West cliff", Vec3::new(-90.0, -8.0, 0.0), 2), ("Behind", Vec3::new(0.0, 10.0, 90.0), 1), ("None", Vec3::ZERO, 0)];
        fill_threats(&mut s, &cam, (1280, 720), &groups);
        let rows = s.list("threats").unwrap();
        assert_eq!(rows.len(), 2, "the visible face and the empty group have no arrow");
    }

    #[test]
    fn the_screens_load_with_the_theme() {
        // Fonts are the system's; without one the HUD is off (and says so), which is fine here.
        let hud = Hud::new(None, crate::controls::bindings().unwrap());
        if let Some(p) = &hud.problem {
            assert!(p.contains("font"), "only a missing font may turn the HUD off: {p}");
        }
    }
}

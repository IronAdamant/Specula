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

use crate::looks::palette;
use sepulcra::content::{Attack, Targets};
use sepulcra::{Content, Outcome};
use specula::game::Overlay;
use specula::sight::Camera;
use specula_core::math::{project_to_screen, Vec3};
use specula_core::Json;
use specula_input::{ActionMap, Input};
use specula_text::{find_system_ui_font, load_font_file};
use specula_ui::gpu::GpuOverlay;
use specula_ui::{FontSet, StateMap, Theme, ToastKind, Ui, UiEvents, Viewport};
use std::path::Path;

/// The screens the game uses, with built-in copies for when the assets folder lacks them.
const SCREENS: [(&str, &str); 7] = [
    ("hud", include_str!("../../sepulcra/assets/ui/hud.json")),
    ("radial", include_str!("../../sepulcra/assets/ui/radial.json")),
    ("pause", include_str!("../../sepulcra/assets/ui/pause.json")),
    ("victory", include_str!("../../sepulcra/assets/ui/victory.json")),
    ("defeat", include_str!("../../sepulcra/assets/ui/defeat.json")),
    ("settings", include_str!("../../sepulcra/assets/ui/settings.json")),
    ("controls", include_str!("../../sepulcra/assets/ui/controls.json")),
];
const THEME: &str = include_str!("../../sepulcra/assets/ui/theme.json");
const CONFIRM: &str = include_str!("../../sepulcra/assets/ui/confirm.json");

/// Most widgets `play_ui` lists.
pub const MAX_TREE_WIDGETS: usize = 200;

/// Interface sizes the settings cycle through.
pub const UI_SCALES: [f32; 5] = [0.8, 0.9, 1.0, 1.15, 1.3];

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
}

/// Seconds a message stays on a capture.
const CAPTURE_TOAST_S: f32 = 4.0;
/// Most toasts a capture shows.
const CAPTURE_TOASTS: usize = 3;

/// The UI's viewport for an image of `size` at the player's interface size.
fn viewport(ui: &Ui, size: (u32, u32), user_scale: f32) -> Viewport {
    let size = [size.0.max(1), size.1.max(1)];
    Viewport::new(size, ui.theme().scale.scale(size, None, user_scale))
}

fn read(dir: Option<&Path>, name: &str, builtin: &'static str) -> String {
    dir.and_then(|d| std::fs::read_to_string(d.join(format!("{name}.json"))).ok()).unwrap_or_else(|| builtin.to_owned())
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
        let (path, font) = find_system_ui_font().ok_or("no system UI font found")?;
        let mut fonts = FontSet::new().with("ui", font);
        if let Some(fonts_dir) = path.parent() {
            for (slot, files) in [("ui_bold", ["segoeuib.ttf", "arialbd.ttf"]), ("title", ["georgiab.ttf", "constanb.ttf"])] {
                if let Some(f) = files.iter().find_map(|f| load_font_file(fonts_dir.join(f)).ok()) {
                    fonts.add(slot, f);
                }
            }
        }
        let theme = Theme::from_json_str(&read(dir, "theme", THEME)).map_err(|e| format!("ui/theme.json: {e}"))?;
        let mut ui = Ui::new(theme, fonts);
        for (name, builtin) in SCREENS.iter().chain([("confirm", CONFIRM)].iter()) {
            ui.load_screen(&read(dir, name, builtin)).map_err(|e| format!("ui/{name}.json: {e}"))?;
        }
        ui.show("hud").map_err(|e| e.to_string())?;
        Ok(ui)
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
        }
    }

    pub fn enabled(&self) -> bool {
        self.ui.is_some()
    }

    /// One window frame: input in, layout, events out (what the player pressed, what the UI took).
    pub fn frame(&mut self, input: &Input, size: (u32, u32)) -> UiEvents {
        let Some(ui) = self.ui.as_mut() else { return UiEvents::default() };
        if size.0 == 0 || size.1 == 0 {
            return UiEvents::default();
        }
        let view = viewport(ui, size, self.user_scale);
        self.laid_out = Some(size);
        ui.frame(input, &self.state, &view).events.clone()
    }

    /// The overlay quads for one image: the window (laid out by [`Hud::frame`] this frame) or a
    /// capture (laid out here for its size, with no input).
    /// `now`: presentation seconds (which messages a capture still shows).
    pub fn overlay(&mut self, out: &mut Overlay, now: f32) {
        let Some(ui) = self.ui.as_mut() else { return };
        if out.capture {
            ui.clear_toasts();
            let fresh: Vec<&(f32, String, ToastKind)> = self.recent.iter().filter(|(t, _, _)| now - *t < CAPTURE_TOAST_S && *t <= now).collect();
            for (_, text, kind) in fresh.iter().skip(fresh.len().saturating_sub(CAPTURE_TOASTS)) {
                ui.toast(text, *kind);
            }
        }
        if out.capture || self.laid_out != Some(out.size) {
            let view = viewport(ui, out.size, self.user_scale);
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
        let view = viewport(ui, self.laid_out.unwrap_or((1280, 720)), self.user_scale);
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
        let action = events.actions().next().map(str::to_owned);
        match action {
            Some(a) => Ok(Some(a)),
            None => Err(format!("pressing '{id}' sent no action")),
        }
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
    pub hover: &'a str,
    /// The selected cell's stone (kind index, tier) or wall (hp, max hp), with what upgrading
    /// costs and selling gives back.
    pub inspect: Option<Inspect>,
    pub kills: u32,
    pub run_time_s: f64,
    pub outcome: Outcome,
    pub volume: f32,
    pub reduced_motion: bool,
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

/// One line on what a stone does at a tier.
pub fn stone_detail(content: &Content, kind: u8, tier: u8) -> String {
    let def = &content.stones[usize::from(kind)];
    let Some(t) = def.tiers.get(usize::from(tier)) else { return String::new() };
    let range = t.range as f64 / 1000.0;
    let dmg = t.damage as f64 / 1000.0;
    match def.attack {
        Attack::Bolt => format!("Homing bolts: {dmg} damage every {:.1} s, {range} m range.", secs(t.cooldown)),
        Attack::Lob => format!("Lobbed fire: {dmg} damage in {} m, every {:.1} s, {range} m range.", t.splash as f64 / 1000.0, secs(t.cooldown)),
        Attack::Pulse => format!("Slows {} % for {:.1} s and deals {dmg} damage every {:.1} s, {range} m.", t.slow / 10, secs(t.slow_ticks), secs(t.cooldown)),
    }
}

fn role(content: &Content, kind: u8) -> &'static str {
    let def = &content.stones[usize::from(kind)];
    match (def.attack, def.targets) {
        (_, Targets::Air) => "Anti-air · flyers only",
        (Attack::Pulse, _) => "Area slow · ground",
        (Attack::Lob, _) => "Splash fire · ground",
        _ => "Single target · ground",
    }
}

const TIERS: [&str; 3] = ["I", "II", "III"];

/// Fills `s` from `info` (everything the screens bind to; see their `notes`).
pub fn fill(s: &mut StateMap, info: &HudInfo) {
    let content = info.content;
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
        let cost = def.tiers[0].cost;
        s.set_bool(&format!("sel_{icon}"), info.selected == Some(i as u8));
        s.set_int(&format!("cost_{icon}"), i64::from(cost));
        s.set_bool(&format!("can_{icon}"), info.dust >= cost);
    }
    let wall_cost = content.rules.wall.cost;
    s.set_bool("sel_wall", info.selected.is_none()).set_int("cost_wall", i64::from(wall_cost)).set_bool("can_wall", info.dust >= wall_cost);
    match info.selected.filter(|&i| usize::from(i) < content.stones.len()) {
        Some(i) => {
            let def = &content.stones[usize::from(i)];
            s.set_str("selected", icon_of(&def.id));
            s.set_str("selected_name", &format!("{} I", def.name));
            s.set_str("selected_role", role(content, i));
            s.set_str("selected_detail", &stone_detail(content, i, 0));
            s.set_int("selected_cost", i64::from(def.tiers[0].cost)).set_bool("affordable", info.dust >= def.tiers[0].cost);
        }
        None => {
            s.set_str("selected", "wall").set_str("selected_name", "Stone wall").set_str("selected_role", "Blocks a lane · reroutes the dead");
            s.set_str("selected_detail", "Walls force the dead onto longer routes. With no route left they break the nearest wall.");
            s.set_int("selected_cost", i64::from(wall_cost)).set_bool("affordable", info.dust >= wall_cost);
        }
    }
    if !info.hover.is_empty() {
        s.set_str("hover", info.hover);
    }
    match info.inspect {
        Some(Inspect::Stone { kind, tier, upgrade, refund }) => {
            let def = &content.stones[usize::from(kind)];
            s.set_str("inspect", &format!("{} {}", def.name, TIERS[usize::from(tier.min(2))]));
            s.set_str("inspect_icon", icon_of(&def.id));
            s.set_str("inspect_detail", &stone_detail(content, kind, tier));
            if let Some(cost) = upgrade {
                s.set_int("upgrade_cost", i64::from(cost)).set_bool("can_upgrade", info.dust >= cost);
                s.set_str("upgrade_detail", &format!("Tier {}: {}", TIERS[usize::from(tier.min(1)) + 1], stone_detail(content, kind, tier + 1)));
            } else {
                s.set_str("upgrade_detail", "Fully carved.");
            }
            s.set_int("refund", i64::from(refund)).set_bool("inspect_stone", true);
        }
        Some(Inspect::Wall { hp, max_hp, refund }) => {
            s.set_str("inspect", "Stone wall").set_str("inspect_icon", "wall");
            s.set_str("inspect_detail", &format!("{} / {} HP. Removing it refunds what's left of it.", hp / 1000, max_hp / 1000));
            s.set_int("refund", i64::from(refund)).set_bool("inspect_wall", true);
        }
        None => {}
    }
    s.set_int("kills", i64::from(info.kills)).set_num("run_time_s", info.run_time_s);
    // Settings.
    s.set_num("volume", f64::from(info.volume)).set_str("reduced_motion_text", if info.reduced_motion { "On" } else { "Off" });
    s.set_str("glyph_style", info.glyph_style);
}

/// Sets `ui_scale` (the settings screen shows it).
pub fn fill_scale(s: &mut StateMap, scale: f32) {
    s.set_num("ui_scale", f64::from(scale));
}

/// Where threats are: one row per face (or the air) with enemies the camera can't see, for the
/// screen-edge arrows (STYLE.md: grave green, pointing at the face under attack). `groups` are
/// `(label, centre of its enemies, count)`.
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
        row.set_str("label", &format!("{label} ({count})"));
    }
}

/// The toast kind for a message the game says (threats in the enemy's green).
pub fn toast_kind(text: &str) -> ToastKind {
    let t = text.to_ascii_lowercase();
    if t.contains("raised") || t.contains("defeat") || t.contains("broken") {
        ToastKind::Threat
    } else if t.starts_with("can't") || t.contains("need ") {
        ToastKind::Warn
    } else if t.contains("cleared") || t.contains("victory") || t.starts_with('+') || t.starts_with("sold") {
        ToastKind::Good
    } else {
        ToastKind::Info
    }
}

/// The HUD palette check (STYLE.md): threats use the enemy's green, never the player's cyan.
pub const THREAT_COLOR: [f32; 3] = palette::GRAVE_GLOW;

#[cfg(test)]
mod tests {
    use super::*;
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
            hover: "top:12,9",
            inspect: Some(Inspect::Stone { kind: 2, tier: 1, upgrade: Some(110), refund: 112 }),
            kills: 40,
            run_time_s: 200.0,
            outcome: Outcome::Running,
            volume: 0.75,
            reduced_motion: false,
            glyph_style: "Auto",
        }
    }

    #[test]
    fn the_state_has_what_the_screens_bind_to() {
        let content = Content::builtin();
        let mut s = StateMap::new();
        fill(&mut s, &info(&content));
        for key in ["wave", "waves", "next_wave_s", "dust", "tombstones", "tombstones_max", "selected", "selected_cost", "affordable", "hover"] {
            assert!(s.get(key).is_some(), "{key} missing");
        }
        assert!(matches!(s.get("sel_warding"), Some(specula_ui::Value::Bool(true))));
        assert!(matches!(s.get("can_brazier"), Some(specula_ui::Value::Bool(false))), "55 dust can't buy an 80-dust brazier");
        assert!(matches!(s.get("inspect"), Some(specula_ui::Value::Str(t)) if t == "Brazier cairn II"), "{:?}", s.get("inspect"));
        assert!(matches!(s.get("can_upgrade"), Some(specula_ui::Value::Bool(false))));
        // Nothing to inspect, no wave timer: the keys are gone (the panels hide).
        fill(&mut s, &HudInfo { inspect: None, next_wave_s: None, ..info(&content) });
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

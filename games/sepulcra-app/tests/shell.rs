//! Sepulcra's shell (lane m5-a11y): the title screen, saved settings and whose changes are saved,
//! rebinding (the player's own input only), gamepad pause, subtitles and captions, handheld
//! profiles, and the accessibility settings passing `ui_check` at every setting. Needs a system
//! UI font for the HUD parts; without one those say so and pass.

use sepulcra::Content;
use sepulcra_app::settings::{Settings, Vision, TEXT_SIZES, UI_SCALES};
use sepulcra_app::SepulcraGame;
use specula::game::{Clock, Ctx, Frame, Game, GameWorld, SettingsStore, TimelineEvent, TimelineWhat, UiPick};
use specula::world::World;
use specula_core::Json;
use specula_platform::{Event, GamepadButton, GamepadState, MouseButton, RawGamepad};
use specula_ui::check::{check_with, CheckOpts, Severity, Target};
use specula_ui::{Display, Panel};
use std::sync::Arc;

struct Host {
    game: SepulcraGame,
    world: World,
    clock: Clock,
    size: (u32, u32),
    windowed: bool,
    /// The window's camera slot, kept across frames as the engine keeps it (a fresh empty slot
    /// every frame would reset the game's orbit each time, and a camera assertion would test
    /// nothing: review R1 of m5-a11y).
    cam: Option<specula::sight::Camera>,
}

fn text(out: &specula_protocol::ToolOutput) -> String {
    match &out.content[0] {
        specula_protocol::Content::Text(t) => t.clone(),
        _ => String::new(),
    }
}

impl Host {
    /// A started game: `windowed` (the title shows), with these saved settings in `store`.
    fn new(windowed: bool, settings: Option<&str>, store: SettingsStore) -> Option<Host> {
        let mut game = SepulcraGame::new(Arc::new(Content::builtin()), 7);
        let mut world = World::default();
        let clock = Clock::new(game.tick_hz());
        game.use_settings(settings, &store);
        game.start(&mut Ctx { world: GameWorld::new(&mut world), clock: &clock, agent: "sepulcra", windowed });
        game.display(&Display::new([1280, 800]));
        if game.ui(UiPick::Live).is_none() {
            println!("SKIP: the HUD is off (no system UI font?)");
            return None;
        }
        Some(Host { game, world, clock, size: (1280, 800), windowed, cam: None })
    }

    fn ctx_call(&mut self, name: &str, args: &str) -> (bool, Json) {
        let Host { game, world, clock, windowed, .. } = self;
        let out = game.call_tool(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: *windowed }, name, &Json::parse(args).unwrap());
        let t = text(&out);
        (out.is_error, Json::parse(&t).unwrap_or(Json::Str(t)))
    }

    /// A window frame (`synthetic`: an agent's ui_act frame) with these events and pad buttons.
    fn frame(&mut self, events: &[Event], pad: Option<u32>, synthetic: bool) {
        let Host { game, world, clock, size, windowed, cam } = self;
        let mut title = None;
        let pads: Vec<GamepadState> = pad.map(|b| GamepadState { connected: true, buttons: b, raw: RawGamepad { buttons: b, ..RawGamepad::default() }, ..GamepadState::default() }).into_iter().collect();
        let mut f = Frame { input: events, size: *size, alpha: 1.0, dt_s: 0.016, camera: cam, agent_view: false, title: &mut title, pads: &pads, synthetic };
        game.frame(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: *windowed }, &mut f);
    }

    /// A key pressed and let go (two frames): the window's own, or an agent's (`synthetic`).
    fn key(&mut self, name: &str, synthetic: bool) {
        let k = specula_platform::keys::from_name(name).unwrap();
        self.frame(&[Event::KeyDown { code: k, repeat: false }], None, synthetic);
        self.frame(&[Event::KeyUp { code: k }], None, synthetic);
    }

    /// What is live (play_settings).
    fn live(&mut self) -> Json {
        let (err, j) = self.ctx_call("play_settings", "{}");
        assert!(!err, "{j:?}");
        j.get("live").cloned().unwrap()
    }

    /// The player opens a screen from the pause menu with their own input (Escape, a click).
    fn player_opens(&mut self, button: &str, screen: &str) {
        self.frame(&[], None, false);
        for _ in 0..2 {
            if self.top() == "pause" {
                break;
            }
            self.key("Escape", false);
        }
        assert_eq!(self.top(), "pause");
        self.click(button);
        assert_eq!(self.top(), screen);
    }

    /// The camera the game last chose, as text (to compare two frames).
    fn camera(&self) -> String {
        format!("{:?}", self.cam.as_ref().map(|c| (c.eye, c.target)))
    }

    fn tick(&mut self) {
        let Host { game, world, clock, windowed, .. } = self;
        game.tick(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: *windowed });
        clock.finish_tick();
    }

    fn top(&mut self) -> String {
        let g = self.game.ui(UiPick::Live).expect("on");
        g.ui.visible_screens().filter(|s| *s != "subtitles").last().unwrap_or("").to_owned()
    }

    /// Clicks a shown widget's centre with the window's own mouse (two frames).
    fn click(&mut self, id: &str) {
        let (x, y) = {
            let g = self.game.ui(UiPick::Live).expect("on");
            let r = g.ui.widget_shown(id).and_then(|w| g.ui.rect(w)).unwrap_or_else(|| panic!("{id} isn't shown"));
            let at = ((r.x + r.w / 2.0) as i32, (r.y + r.h / 2.0) as i32);
            let hit = g.ui.hit_test([at.0 as f32, at.1 as f32]).and_then(|w| g.ui.widget_id(w)).map(str::to_owned);
            assert_eq!(hit.as_deref(), Some(id), "the click lands on {id}");
            at
        };
        self.frame(&[Event::MouseMove { x, y }], None, false);
        self.frame(&[Event::MouseDown { button: MouseButton::Left, x, y }], None, false);
        self.frame(&[Event::MouseUp { button: MouseButton::Left, x, y }], None, false);
        self.frame(&[], None, false);
    }

    /// What the screens show for state `key`: its text, or its message in the player's language
    /// (the shell says everything through the string tables).
    fn state_str(&mut self, key: &str) -> Option<String> {
        let g = self.game.ui(UiPick::Live).expect("on");
        match g.state.get(key) {
            Some(specula_ui::Value::Str(s)) => Some(s.clone()),
            Some(specula_ui::Value::Msg(_)) => {
                let mut out = String::new();
                let template = specula_ui::state::Template::parse(&format!("{{{key}}}"), "t").expect("a binding");
                template.render(&specula_ui::state::Scope::new(g.state).with_strings(g.ui.strings().map(|s| &**s)), &mut out);
                Some(out)
            }
            _ => None,
        }
    }

    fn settings(&mut self) -> Json {
        let (err, j) = self.ctx_call("play_settings", "{}");
        assert!(!err, "{j:?}");
        j
    }
}

fn per_user() -> SettingsStore {
    SettingsStore { kind: "per_user", agents_save: false, problem: None }
}

fn folder() -> SettingsStore {
    SettingsStore { kind: "folder", agents_save: true, problem: None }
}

#[test]
fn saved_settings_apply_at_start_and_only_the_players_changes_are_saved() {
    let saved = Settings { ui_scale: Some(4), vision: Vision::Deuteranopia, captions: true, ..Settings::default() }.to_text();
    let Some(mut h) = Host::new(false, Some(&saved), per_user()) else { return };
    let s = h.settings();
    assert_eq!(s.get("live").and_then(|l| l.get("vision")).and_then(Json::as_str), Some("deuteranopia"), "{s:?}");
    assert_eq!(h.game.ui(UiPick::Live).unwrap().user_scale, 1.3, "the saved interface size");
    assert_eq!(h.game.take_settings(), None, "nothing changed yet");
    // An agent's change (play_settings, play_ui) is the session's: the player's file keeps theirs.
    let (err, j) = h.ctx_call("play_settings", r#"{"set": {"text_size": 1.3, "vision": "tritanopia"}}"#);
    assert!(!err, "{j:?}");
    assert!(j.get("unsaved").is_some(), "{j:?}");
    assert_eq!(h.game.take_settings(), None, "an agent's change never reaches the player's file");
    // The player's own change (a real frame's click) is saved, without the agent's.
    h.ctx_call("play_ui", r#"{"press": "pause_button"}"#);
    h.game.ui(UiPick::Live);
    h.frame(&[], None, false);
    // Open the settings with the window's own Escape, then click Reduced motion.
    h.frame(&[Event::KeyDown { code: specula_platform::keys::ESCAPE, repeat: false }], None, false);
    h.frame(&[Event::KeyUp { code: specula_platform::keys::ESCAPE }], None, false);
    if h.top() != "pause" {
        h.frame(&[Event::KeyDown { code: specula_platform::keys::ESCAPE, repeat: false }], None, false);
        h.frame(&[Event::KeyUp { code: specula_platform::keys::ESCAPE }], None, false);
    }
    assert_eq!(h.top(), "pause");
    h.click("settings");
    assert_eq!(h.top(), "settings");
    h.click("motion");
    let text = h.game.take_settings().expect("the player's change is saved");
    let file = Settings::parse(&text).settings;
    assert!(file.reduced_motion, "the player's change");
    assert_eq!(file.vision, Vision::Deuteranopia, "not the agent's");
    assert_eq!(file.text_size, None, "not the agent's");
    // A folder the operator named saves agents' changes too.
    let Some(mut h) = Host::new(false, None, folder()) else { return };
    let (err, _) = h.ctx_call("play_settings", r#"{"set": {"vision": "high_contrast"}}"#);
    assert!(!err);
    assert_eq!(Settings::parse(&h.game.take_settings().expect("saved")).settings.vision, Vision::HighContrast);
    // Bad values change nothing and say what to fix.
    for bad in [r#"{"set": {"ui_scale": 7}}"#, r#"{"set": {"bogus": 1}}"#, r#"{"bind": [{"action": "call_wave", "device": "kbm", "input": "Pad.North"}]}"#, r#"{"bind": [{"action": "orbit", "device": "kbm", "input": "Key.G"}]}"#, r#"{"nope": 1}"#] {
        let (err, j) = h.ctx_call("play_settings", bad);
        assert!(err, "{bad}: {j:?}");
    }
    assert_eq!(h.game.take_settings(), None, "refusals change nothing");
}

#[test]
fn the_title_holds_the_run_until_the_player_or_an_agent_acts() {
    let Some(mut h) = Host::new(true, None, per_user()) else { return };
    h.frame(&[], None, false);
    assert_eq!(h.top(), "title");
    let tick = h.game.state().tick();
    for _ in 0..30 {
        h.tick();
    }
    assert_eq!(h.game.state().tick(), tick, "the title holds the run");
    // Quit from an agent is refused; New run from the title starts the intro.
    let (err, _) = h.ctx_call("play_ui", r#"{"press": "quit_game"}"#);
    assert!(!err);
    assert!(!h.game.take_quit(), "agents don't close the human's window");
    let (err, j) = h.ctx_call("play_ui", r#"{"press": "start"}"#);
    assert!(!err, "{j:?}");
    assert_ne!(h.top(), "title");
    assert!(h.ctx_call("play_state", "{}").1.get("cutscene").and_then(|c| c.get("playing")).is_some_and(|p| !p.is_null()), "the intro follows the title");
    // An agent's acting call takes the title away (agents are never held).
    let Some(mut h) = Host::new(true, None, per_user()) else { return };
    h.frame(&[], None, false);
    assert_eq!(h.top(), "title");
    h.ctx_call("play_command", r#"{"action": "call_wave"}"#);
    assert_ne!(h.top(), "title");
    // The player's own Quit asks the engine to stop.
    let Some(mut h) = Host::new(true, None, per_user()) else { return };
    h.frame(&[], None, false);
    h.click("quit_game");
    assert!(h.game.take_quit());
    // Headless: no title.
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    h.frame(&[], None, false);
    assert_eq!(h.top(), "hud");
}

#[test]
fn start_pauses_and_resumes_and_a_lost_pad_pauses() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    let start = GamepadButton::Start.bit();
    h.frame(&[], Some(0), false);
    h.frame(&[], Some(start), false);
    h.frame(&[], Some(0), false);
    assert_eq!(h.top(), "pause", "Start opens the menu");
    h.frame(&[], Some(start), false);
    h.frame(&[], Some(0), false);
    assert_eq!(h.top(), "hud", "Start again closes it");
    assert_eq!(h.ctx_call("play_state", "{}").1.get("paused").and_then(Json::as_bool), Some(false), "and resumes");
    // The pad goes away while in use: paused, with the menu.
    h.frame(&[], Some(GamepadButton::South.bit()), false);
    h.frame(&[], Some(0), false);
    h.frame(&[], None, false);
    assert_eq!(h.top(), "pause");
    assert_eq!(h.ctx_call("play_state", "{}").1.get("paused").and_then(Json::as_bool), Some(true));
    // An agent's synthetic frame without pads isn't a pad going away.
    h.frame(&[], Some(start), false);
    h.frame(&[], Some(0), false);
    assert_eq!(h.top(), "hud");
    h.frame(&[], None, true);
    assert_eq!(h.top(), "hud");
}

#[test]
fn rebinding_listens_to_the_player_only_and_shows_conflicts() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    h.frame(&[], None, false);
    h.ctx_call("play_ui", r#"{"press": "menu_button"}"#);
    h.ctx_call("play_ui", r#"{"press": "controls"}"#);
    assert_eq!(h.top(), "controls");
    // An agent's press can't make the game listen for the player's next key.
    let (err, j) = h.ctx_call("play_ui", r#"{"press": "bind_kbm[0]"}"#);
    assert!(!err, "{j:?}");
    assert!(h.state_str("rebind_status").unwrap_or_default().contains("agents set bindings"), "{:?}", h.state_str("rebind_status"));
    assert_eq!(h.settings().get("rebinding").and_then(Json::as_bool), Some(false));
    // The player clicks the first row's keyboard-and-mouse input (place: the left button) and
    // presses G.
    let row = 0;
    h.click("reset_bindings");
    assert_eq!(h.state_str("rebind_status").as_deref(), Some("Controls reset to the game's own"), "the player's click on Reset");
    h.click(&format!("bind_kbm[{row}]"));
    assert_eq!(h.settings().get("rebinding").and_then(Json::as_bool), Some(true));
    let g = specula_platform::keys::from_name("G").unwrap();
    h.frame(&[Event::KeyDown { code: g, repeat: false }], None, false);
    h.frame(&[Event::KeyUp { code: g }], None, false);
    assert!(h.state_str("rebind_status").unwrap_or_default().starts_with("Bound G"), "{:?}", h.state_str("rebind_status"));
    let saved = Settings::parse(&h.game.take_settings().expect("saved")).settings;
    assert!(saved.bindings.expect("bindings").to_string().contains("Key.G"));
    // Binding by name onto an input another action uses works and names the clash.
    let (err, j) = h.ctx_call("play_settings", r#"{"bind": [{"action": "call_wave", "device": "kbm", "input": "Key.U"}]}"#);
    assert!(!err, "{j:?}");
    assert!(j.get("conflicts").map(|c| c.to_string().contains("upgrade")).unwrap_or(false), "{j:?}");
    // An agent's reset (for the session) goes back to the player's bindings: theirs stay, the
    // agent's go.
    let (err, j) = h.ctx_call("play_settings", r#"{"reset_bindings": true}"#);
    let live = j.get("live").and_then(|l| l.get("bindings"));
    assert!(!err && bound(live, "place").contains("Key.G") && !bound(live, "call_wave").contains("Key.U"), "{j:?}");
}

#[test]
fn subtitles_and_their_settings_reach_the_screen() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    let line = specula_tempus::Line { t: 0.0, duration: 4.0, text: "The dead rise at nightfall.".into(), speaker: Some("Warden".into()) };
    {
        let Host { game, world, clock, .. } = &mut h;
        game.timeline_event(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, &TimelineEvent { path: "assets/timelines/intro.json".into(), t: 0.0, what: TimelineWhat::Subtitle(line) });
    }
    h.frame(&[], None, false);
    assert_eq!(h.state_str("subtitle_text").as_deref(), Some("The dead rise at nightfall."));
    assert_eq!(h.state_str("subtitle_speaker").as_deref(), Some("Warden"));
    let shown = {
        let g = h.game.ui(UiPick::Live).unwrap();
        g.ui.is_visible("subtitles")
    };
    assert!(shown);
    h.ctx_call("play_settings", r#"{"set": {"subtitles": false}}"#);
    h.frame(&[], None, false);
    assert_eq!(h.state_str("subtitle_text"), None, "off: no line");
}

#[test]
fn a_handheld_gets_a_legible_default_the_player_can_still_shrink() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    let deck = Display { panel: Some(Panel { size: [1280, 800], diagonal_in: 7.0, touch: true }), ..Display::new([1280, 800]) };
    h.game.display(&deck);
    let s = h.settings();
    assert_eq!(s.get("handheld").and_then(Json::as_str), Some("Steam Deck"), "{s:?}");
    assert_eq!(h.game.ui(UiPick::Live).unwrap().user_scale, 1.15, "the Deck's legible default");
    // The player may still choose smaller (their choice, not a floor).
    h.ctx_call("play_settings", r#"{"set": {"ui_scale": 0.8}}"#);
    assert_eq!(h.game.ui(UiPick::Live).unwrap().user_scale, 0.8);
    // A desktop gets 1.
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    h.game.display(&Display::new([1920, 1080]));
    assert_eq!(h.game.ui(UiPick::Live).unwrap().user_scale, 1.0);
    assert_eq!(h.settings().get("handheld"), Some(&Json::Null));
}

/// The access screen and the subtitles pass ui_check at every interface size, text size and
/// colour theme, on the Deck, 1080p, 4K and a 175% laptop window (the player's smaller settings,
/// under 1, are exempt from the Deck's legibility floor: their choice).
#[test]
fn every_accessibility_setting_passes_ui_check() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    let line = specula_tempus::Line { t: 0.0, duration: 30.0, text: "The dead rise at nightfall. Carve stones and hold the graveyard.".into(), speaker: Some("Warden".into()) };
    {
        let Host { game, world, clock, .. } = &mut h;
        game.timeline_event(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, &TimelineEvent { path: "t".into(), t: 0.0, what: TimelineWhat::Subtitle(line) });
    }
    let laptop = Target { os_scale: Some(1.75), ..Target::new("window_1280x720_os175", [1280, 720], None) };
    let mut errors = Vec::new();
    for vision in ["standard", "deuteranopia", "protanopia", "tritanopia", "high_contrast"] {
        for (ti, &ts) in TEXT_SIZES.iter().enumerate() {
            // Text 1.5 with the largest interface: the hardest combination; every text size at 1.
            let scales: Vec<f32> = if ti == TEXT_SIZES.len() - 1 { UI_SCALES.to_vec() } else { vec![1.0] };
            let (err, j) = h.ctx_call("play_settings", &format!(r#"{{"set": {{"vision": "{vision}", "text_size": {ts}, "subtitle_size": "huge"}}}}"#));
            assert!(!err, "{j:?}");
            h.frame(&[], None, false);
            let targets: Vec<Target> = scales.iter().flat_map(|&s| [Target::deck(), Target::desktop_1080p(), Target::desktop_4k(), laptop.clone()].map(|t| t.at_scale(s))).collect();
            let smaller: Vec<String> = targets.iter().filter(|t| t.user_scale < 1.0 || ts < 1.0).map(|t| t.name.clone()).collect();
            for set in [&["hud", "subtitles"][..], &["hud", "pause", "settings"], &["hud", "access"], &["hud", "pause", "controls"], &["title"]] {
                let g = h.game.ui(UiPick::Live).expect("on");
                let f = check_with(g.ui, set, g.state, g.input_map, &targets, &CheckOpts::default()).unwrap_or_else(|e| panic!("{set:?}: {e}"));
                errors.extend(
                    f.into_iter()
                        .filter(|f| f.severity == Severity::Error && !(f.rule == "legibility" && smaller.contains(&f.target)))
                        .map(|f| format!("{vision} text {ts} {set:?} [{}] {} {}:{} {}", f.rule, f.target, f.screen, f.widget, f.message)),
                );
            }
        }
    }
    errors.dedup();
    assert!(errors.is_empty(), "{} errors:\n{}", errors.len(), errors.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn vision_themes_keep_meaning_apart() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    for (vision, apart) in [("standard", false), ("deuteranopia", true), ("protanopia", true), ("tritanopia", true)] {
        h.ctx_call("play_settings", &format!(r#"{{"set": {{"vision": "{vision}"}}}}"#));
        let c = h.settings().get("colours").cloned().expect("colours");
        if apart {
            assert_eq!(c.get("pairs_apart").and_then(Json::as_bool), Some(true), "{vision}: {c:?}");
        }
    }
    // The standard palette is what the themes fix: green and yellow merge for deutans.
    h.ctx_call("play_settings", r#"{"set": {"vision": "standard"}}"#);
    let theme = h.game.ui(UiPick::Live).unwrap().ui.theme().clone();
    let c = specula_ui::vision::confusable(&theme, &[("grave", "fire")], &[specula_ui::vision::Deficiency::Deutan], 30.0);
    assert_eq!(c.len(), 1, "{c:?}");
}

// ---- review R1 of m5-a11y: every probe that failed, kept ----

/// Finding 1. The player starts a rebind with their own click; an agent's `ui_act` frame presses
/// G. It used to become the player's binding at the next window frame, saved to their file. An
/// agent's Escape cancelled their capture the same way.
#[test]
fn an_agents_key_never_answers_or_cancels_the_players_rebind() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    h.player_opens("controls", "controls");
    h.click("bind_kbm[0]");
    let listening = |h: &mut Host| h.ctx_call("play_settings", "{}").1.get("rebinding").and_then(Json::as_bool);
    assert_eq!(listening(&mut h), Some(true), "the player's rebind listens");
    let refused = |h: &mut Host| h.ctx_call("play_settings", "{}").1.get("refused_agent_presses").and_then(Json::as_u64).unwrap_or(0);
    let before = refused(&mut h);
    h.key("G", true);
    h.frame(&[], None, false);
    assert_eq!(listening(&mut h), Some(true), "an agent's key didn't end it");
    assert_eq!(h.game.take_settings(), None, "and nothing was saved");
    assert!(refused(&mut h) > before, "the dropped presses are counted (play_settings: refused_agent_presses)");
    h.key("Escape", true);
    h.frame(&[], None, false);
    assert_eq!(listening(&mut h), Some(true), "nor did an agent's Escape cancel it");
    // The player's own key answers, and that is what is saved.
    h.key("H", false);
    let saved = h.game.take_settings().expect("the player's rebind is saved");
    assert!(saved.contains("Key.H") && !saved.contains("Key.G"), "{saved}");
}

/// Finding 3. An agent turns reduced motion on (for the session). The player then toggles it on
/// the settings screen and sees it go off: the file used to save the opposite (each of live and
/// saved was flipped from its own value). Every toggle and cycle was the same.
#[test]
fn a_players_toggle_after_an_agents_change_saves_what_the_player_sees() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    let (err, j) = h.ctx_call("play_settings", r#"{"set": {"reduced_motion": true, "subtitles": false, "captions": true, "volume": 0.5}}"#);
    assert!(!err, "{j:?}");
    assert_eq!(h.game.take_settings(), None, "the agent's change is for the session only");
    h.player_opens("settings", "settings");
    h.click("motion");
    let live = h.live();
    let file = Settings::parse(&h.game.take_settings().expect("the player's change is saved")).settings;
    assert_eq!(live.get("reduced_motion").and_then(Json::as_bool), Some(false), "the player sees it go off");
    assert!(!file.reduced_motion, "and the file holds what they see");
    // What the agent set and the player didn't touch stays out of the file.
    assert!(file.subtitles && !file.captions, "the agent's other session changes aren't saved: {file:?}");
    // The cycles too: the volume the player lands on is the one saved.
    h.click("volume");
    let live = h.live();
    let file = Settings::parse(&h.game.take_settings().expect("saved")).settings;
    assert_eq!(live.get("volume").and_then(Json::as_f64).map(|v| (v * 100.0).round()), Some((f64::from(file.volume()) * 100.0).round()), "live {live:?}");
}

/// Finding 2. An agent binds call_wave to G by name (for the session). The player then rebinds
/// another action with their own input: the whole live map, the agent's binding included, used
/// to be written to the player's file.
#[test]
fn an_agents_session_binding_never_rides_into_the_players_file() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    let (err, j) = h.ctx_call("play_settings", r#"{"bind": [{"action": "call_wave", "device": "kbm", "input": "Key.G"}]}"#);
    assert!(!err, "{j:?}");
    assert_eq!(h.game.take_settings(), None);
    h.player_opens("controls", "controls");
    h.click("bind_kbm[0]");
    h.key("H", false);
    let text = h.game.take_settings().expect("the player's rebind is saved");
    let b = Settings::parse(&text).settings.bindings.map(|b| b.to_string()).unwrap_or_default();
    assert!(b.contains("Key.H"), "the player's binding: {b}");
    assert!(!b.contains("Key.G"), "the agent's session-only binding reached the player's file: {b}");
    // Live keeps both for the session.
    let live = h.live().get("bindings").map(Json::to_string).unwrap_or_default();
    assert!(live.contains("Key.H") && live.contains("Key.G"), "{live}");
}

/// Finding 4 (the lead's watch item: "Start opens the pause menu" failed whenever another window
/// was in front). The window loses focus; an agent's `ui_act` then presses Start on a gamepad.
#[test]
fn an_agents_gamepad_press_works_while_the_window_is_in_the_background() {
    let start = GamepadButton::Start.bit();
    for unfocused in [false, true] {
        let Some(mut h) = Host::new(false, None, per_user()) else { return };
        h.frame(if unfocused { &[Event::Focused(false)] } else { &[] }, None, false);
        h.frame(&[], Some(0), true);
        h.frame(&[], Some(start), true);
        h.frame(&[], Some(0), true);
        assert_eq!(h.top(), "pause", "an agent's Start opens the menu (window unfocused: {unfocused})");
    }
}

/// Finding 7. An agent rebinds the keyboard's ways into the pause menu to keys no keyboard has:
/// the player's Escape no longer opened it. Those two are the player's to rebind.
#[test]
fn an_agent_cant_bind_the_players_menu_keys_away() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    for (action, key) in [("cancel", "Key.F24"), ("pause", "Key.F23")] {
        let (err, j) = h.ctx_call("play_settings", &format!(r#"{{"bind": [{{"action": "{action}", "device": "kbm", "input": "{key}"}}]}}"#));
        assert!(err && j.get("error").and_then(Json::as_str) == Some("players_action"), "{action}: {j:?}");
        let (err, j) = h.ctx_call("play_settings", &format!(r#"{{"bind": [{{"action": "{action}", "device": "pad", "input": "Pad.North"}}]}}"#));
        assert!(err && j.get("error").and_then(Json::as_str) == Some("players_action"), "{action} on a pad: {j:?}");
    }
    // One refused entry refuses the whole call: nothing else in it was bound.
    let (err, _) = h.ctx_call("play_settings", r#"{"bind": [{"action": "call_wave", "device": "kbm", "input": "Key.G"}, {"action": "cancel", "device": "kbm", "input": "Key.F24"}]}"#);
    assert!(err);
    assert!(!h.live().get("bindings").map(Json::to_string).unwrap_or_default().contains("Key.G"));
    h.frame(&[], None, false);
    h.key("Escape", false);
    assert_eq!(h.top(), "pause", "the player's Escape opens the pause menu");
}

/// Finding 8. On a desktop Auto gives 100%; the first press of Settings > Interface size went
/// from Auto to 100% again.
#[test]
fn cycling_the_interface_size_from_auto_goes_a_size_up() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    h.frame(&[], None, false);
    assert_eq!(h.game.ui(UiPick::Live).unwrap().user_scale, 1.0);
    h.player_opens("settings", "settings");
    h.click("ui_scale");
    let after = h.game.ui(UiPick::Live).unwrap().user_scale;
    assert!(after > 1.0, "the first press from Auto (100%) goes a size up; it gave {after}");
}

/// Finding 9. Camera drag on toggle: a right click latches the orbit. The latch used to outlive
/// the pause menu, so the player came back to a camera that swung with every mouse move.
#[test]
fn a_toggled_camera_drag_ends_when_the_menu_opens() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    let (err, j) = h.ctx_call("play_settings", r#"{"set": {"camera_drag": "toggle"}}"#);
    assert!(!err, "{j:?}");
    let (x, y) = (640, 330);
    h.frame(&[Event::MouseMove { x, y }], None, false);
    h.frame(&[Event::MouseDown { button: MouseButton::Right, x, y }], None, false);
    h.frame(&[Event::MouseUp { button: MouseButton::Right, x, y }], None, false);
    h.frame(&[], None, false);
    let a = h.camera();
    h.frame(&[Event::MouseMove { x: x + 40, y }], None, false);
    let b = h.camera();
    assert_ne!(a, b, "control: with toggle, a click latches the orbit and a plain move swings the camera");
    h.key("Escape", false);
    assert_eq!(h.top(), "pause");
    h.key("Escape", false);
    assert_eq!(h.top(), "hud");
    h.frame(&[], None, false);
    let c = h.camera();
    h.frame(&[Event::MouseMove { x: x + 100, y }], None, false);
    assert_eq!(c, h.camera(), "after the menu a plain mouse move leaves the camera alone");
}

// ---- re-check R2 of m5-a11y: its probes, kept ----

/// The row of `action` on the controls screen.
fn row_of(action: &str) -> usize {
    let map = sepulcra_app::controls::bindings().unwrap();
    map.actions().iter().filter(|a| !a.name.starts_with("ui_")).take(24).position(|a| a.name == action).unwrap()
}

/// What `action` is bound to in a bindings object (`{"bindings": {action: [...]}}`), as text.
fn bound(bindings: Option<&Json>, action: &str) -> String {
    bindings.and_then(|b| b.get("bindings")).and_then(|m| m.get(action)).map(Json::to_string).unwrap_or_default()
}

/// A window tall enough to show every row of the controls screen.
fn tall(h: &mut Host) {
    h.size = (1280, 3000);
    h.game.display(&Display::new([1280, 3000]));
    h.frame(&[], None, false);
}

/// The player presses and lets go of a pad button (the window's own frames).
fn pad_press(h: &mut Host, button: GamepadButton) {
    h.frame(&[], Some(0), false);
    h.frame(&[], Some(button.bit()), false);
    h.frame(&[], Some(0), false);
}

/// R2 finding 1, first case. `speed` has no pad binding by default. An agent binds one for the
/// session; the player then rebinds speed's pad button. The slot came from the live map
/// ("replace the second binding"), the saved map had one, so nothing was saved while the screen
/// said "bound".
#[test]
fn a_players_rebind_is_saved_when_an_agent_bound_the_same_action_first() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    tall(&mut h);
    let (err, j) = h.ctx_call("play_settings", r#"{"bind": [{"action": "speed", "device": "pad", "input": "Pad.North"}]}"#);
    assert!(!err, "{j:?}");
    assert_eq!(h.game.take_settings(), None);
    h.player_opens("controls", "controls");
    h.click(&format!("bind_pad[{}]", row_of("speed")));
    assert_eq!(h.ctx_call("play_settings", "{}").1.get("rebinding").and_then(Json::as_bool), Some(true));
    pad_press(&mut h, GamepadButton::West);
    let saved = h.game.take_settings().expect("the player's pad rebind of speed is saved");
    let b = bound(Settings::parse(&saved).settings.bindings.as_ref(), "speed");
    assert!(b.contains("Pad.West") && !b.contains("Pad.North"), "the player's binding of speed, not the agent's: {b}");
}

/// R2 finding 1, second case, and finding 4. The player rebinds speed's pad button; an agent
/// resets the bindings for the session; the player rebinds it again. The file used to hold both
/// of the player's bindings (the second was appended beside the first), and the agent's reset
/// used to take the player's own remap from them for the session.
#[test]
fn an_agents_reset_goes_back_to_the_players_bindings_and_a_rebind_after_it_replaces() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    tall(&mut h);
    h.player_opens("controls", "controls");
    let row = row_of("speed");
    h.click(&format!("bind_pad[{row}]"));
    pad_press(&mut h, GamepadButton::West);
    assert!(h.game.take_settings().is_some_and(|t| t.contains("Pad.West")));
    // The agent also binds something of its own, then resets: the player's remap is still live,
    // the agent's is gone.
    let (err, j) = h.ctx_call("play_settings", r#"{"bind": [{"action": "call_wave", "device": "kbm", "input": "Key.G"}]}"#);
    assert!(!err, "{j:?}");
    let (err, j) = h.ctx_call("play_settings", r#"{"reset_bindings": true}"#);
    assert!(!err, "{j:?}");
    let live = h.live();
    let (speed, wave) = (bound(live.get("bindings"), "speed"), bound(live.get("bindings"), "call_wave"));
    assert!(speed.contains("Pad.West") && !wave.contains("Key.G"), "a reset for the session goes back to the player's bindings: speed {speed}, call_wave {wave}");
    assert_eq!(h.game.take_settings(), None, "and saves nothing");
    // The player rebinds it again: one pad binding in the file, the new one.
    h.click(&format!("bind_pad[{row}]"));
    pad_press(&mut h, GamepadButton::East);
    let saved = h.game.take_settings().expect("saved");
    let b = bound(Settings::parse(&saved).settings.bindings.as_ref(), "speed");
    assert!(b.contains("Pad.East") && !b.contains("Pad.West"), "the new binding of speed replaced the old one: {b}");
    // Where the operator's folder saves agents' changes, a reset is the game's own bindings.
    let Some(mut h) = Host::new(false, Some(&saved), SettingsStore { kind: "folder", agents_save: true, problem: None }) else { return };
    let (err, j) = h.ctx_call("play_settings", r#"{"reset_bindings": true}"#);
    assert!(!err, "{j:?}");
    let file = Settings::parse(&h.game.take_settings().expect("saved")).settings;
    assert_eq!(file.bindings, None, "back to the game's own");
}

/// R2 finding 3, and R1 finding 14. Presses the game refuses as the player's alone are counted
/// for the engine to take (and tell the input monitor): a pad-only press dropped while the
/// player listens for a rebind, an agent's click on a rebind button, an agent's Quit.
#[test]
fn presses_refused_as_the_players_alone_are_counted_for_the_engine() {
    let Some(mut h) = Host::new(false, None, per_user()) else { return };
    tall(&mut h);
    h.player_opens("controls", "controls");
    let row = row_of("speed");
    h.click(&format!("bind_pad[{row}]"));
    let _ = h.game.take_refused_presses();
    h.frame(&[], Some(0), true);
    h.frame(&[], Some(GamepadButton::Start.bit()), true);
    h.frame(&[], Some(0), true);
    assert!(h.game.take_refused_presses() > 0, "a dropped pad press is counted");
    assert_eq!(h.game.take_refused_presses(), 0, "taken once");
    assert_eq!(h.top(), "controls", "and it did nothing (Start would have closed the menu)");
    // An idle synthetic frame (nothing pressed) isn't a refused press.
    h.frame(&[], Some(0), true);
    h.frame(&[], None, true);
    assert_eq!(h.game.take_refused_presses(), 0);
    // The player ends their rebind; an agent then presses a rebind button: refused, counted.
    h.key("Escape", false);
    let (err, j) = h.ctx_call("play_ui", &format!(r#"{{"press": "bind_kbm[{row}]"}}"#));
    assert!(!err || j.get("error").is_some(), "{j:?}");
    assert!(h.game.take_refused_presses() > 0, "an agent's press of a rebind button is counted");
    assert_eq!(h.ctx_call("play_settings", "{}").1.get("rebinding").and_then(Json::as_bool), Some(false), "and nothing listens");
}

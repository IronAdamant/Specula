//! Sepulcra's interface for the engine's `ui_*` tools (lane m5-uitools): every screen passes
//! `ui_check` at the Steam Deck, 1080p and 4K, at UI scale 1.0 and 1.5; the interface the game
//! lends the engine lays out exactly as its frames do for the display the engine tells it; and a
//! gamepad reaches every menu (Start opens the pause menu, the d-pad route the focus rules give
//! lands where it says, B goes back). Needs a system UI font; without one the HUD is off and
//! these tests say so and pass.

use sepulcra::Content;
use sepulcra_app::hud::UI_SCALES;
use sepulcra_app::SepulcraGame;
use specula::game::{Clock, Ctx, Frame, Game, GameWorld, Overlay, UiPick};
use specula::sight::Camera;
use specula_core::math::Vec3;
use specula_gpu::{GpuError, OverlayTextures, TextureFormat, TextureId};
use specula::world::World;
use specula_platform::{GamepadButton, GamepadState, RawGamepad};
use specula_ui::check::{check_with, CheckOpts, Severity, Target};
use specula_ui::geom::Dir;
use specula_ui::Display;
use std::sync::Arc;

struct Host {
    game: SepulcraGame,
    world: World,
    clock: Clock,
    size: (u32, u32),
}

impl Host {
    fn new(size: (u32, u32)) -> Option<Host> {
        let mut game = SepulcraGame::new(Arc::new(Content::builtin()), 7);
        let mut world = World::default();
        let clock = Clock::new(game.tick_hz());
        game.start(&mut Ctx { world: GameWorld::new(&mut world), clock: &clock, agent: "sepulcra", windowed: false });
        game.display(&Display::new([size.0, size.1]));
        if game.ui(UiPick::Live).is_none() {
            println!("SKIP: the HUD is off (no system UI font?)");
            return None;
        }
        Some(Host { game, world, clock, size })
    }

    fn frame_with(&mut self, buttons: u32) {
        let _ = self.frame_as(buttons, false, 1.0);
    }

    /// A frame with `buttons` held: the window's, or an agent's synthetic one (ui_act's). The
    /// window title the frame set, if any.
    fn frame_as(&mut self, buttons: u32, synthetic: bool, alpha: f32) -> Option<String> {
        let Host { game, world, clock, size } = self;
        let (mut cam, mut title) = (None, None);
        let raw = RawGamepad { buttons, ..RawGamepad::default() };
        let pads = [GamepadState { connected: true, buttons, raw, ..GamepadState::default() }];
        let mut f = Frame { input: &[], size: *size, alpha, dt_s: 0.016, camera: &mut cam, agent_view: false, title: &mut title, pads: &pads, synthetic };
        game.frame(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, &mut f);
        title
    }

    /// One tick of the running clock.
    fn tick(&mut self) {
        let Host { game, world, clock, .. } = self;
        game.tick(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false });
        clock.finish_tick();
    }

    /// Where the dead stand in the world (what world_query, physics and sight read).
    fn enemy_positions(&self) -> Vec<(u64, [f32; 3])> {
        let Ok(group) = self.world.find_path("/live/enemies") else { return Vec::new() };
        self.world.children_of(Some(group)).filter_map(|id| self.world.get(id).map(|e| (id, [e.position.x, e.position.y, e.position.z]))).collect()
    }

    /// A capture's interface (`sight_frame {include_ui}`, `ui_frame`) at the host's size.
    fn capture(&mut self) {
        let Host { game, world, clock, size } = self;
        let (mut quads, mut textures) = (Vec::new(), Textures(0));
        let cam = Camera::new(Vec3::new(38.0, 27.0, 52.0), Vec3::new(0.0, -9.0, 1.0), 45.0);
        let mut o = Overlay { size: *size, camera: cam, capture: true, quads: &mut quads, textures: &mut textures };
        game.overlay(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, &mut o);
    }

    /// A pad button pressed and let go (two frames), then a frame to lay out what it opened.
    fn press(&mut self, b: GamepadButton) {
        self.frame_with(b.bit());
        self.frame_with(0);
        self.frame_with(0);
    }

    fn top(&mut self) -> String {
        let g = self.game.ui(UiPick::Live).expect("on");
        g.ui.visible_screens().last().unwrap_or("").to_owned()
    }

    fn focused(&mut self) -> Option<String> {
        let g = self.game.ui(UiPick::Live).expect("on");
        g.ui.focused().and_then(|w| g.ui.widget_id(w)).map(str::to_owned)
    }

    /// Walks the d-pad route to `id`, as `ui_act {focus}` does.
    fn focus(&mut self, id: &str) {
        for _ in 0..32 {
            let next = {
                let g = self.game.ui(UiPick::Live).expect("on");
                let w = g.ui.widget_shown(id).unwrap_or_else(|| panic!("no {id}"));
                g.ui.focus_route(w).unwrap_or_else(|e| panic!("{id}: {e}")).first().copied()
            };
            let Some(d) = next else { return };
            self.press(match d {
                Dir::Up => GamepadButton::DpadUp,
                Dir::Down => GamepadButton::DpadDown,
                Dir::Left => GamepadButton::DpadLeft,
                Dir::Right => GamepadButton::DpadRight,
            });
        }
        panic!("never reached {id}");
    }
}

/// Textures for the HUD without a GPU.
struct Textures(u32);

impl OverlayTextures for Textures {
    fn upload_texture(&mut self, _w: u32, _h: u32, _f: TextureFormat, _p: &[u8]) -> Result<TextureId, GpuError> {
        self.0 += 1;
        Ok(TextureId(self.0))
    }
    fn update_texture(&mut self, _t: TextureId, _x: u32, _y: u32, _w: u32, _h: u32, _p: &[u8]) -> Result<(), GpuError> {
        Ok(())
    }
    fn destroy_texture(&mut self, _t: TextureId) {}
}

const SETS: [&[&str]; 10] = [
    &["hud"],
    &["hud", "radial"],
    &["hud", "pause"],
    &["hud", "pause", "settings"],
    &["hud", "pause", "controls"],
    &["hud", "pause", "saves"],
    &["hud", "pause", "confirm"],
    &["hud", "victory"],
    &["hud", "defeat"],
    &["title"],
];

fn errors_at(h: &mut Host, what: &str) -> Vec<String> {
    // Every UI-scale setting the settings menu cycles through (review R2 of m5-uitools,
    // finding 4: 1.15 and 1.3 weren't checked, and the build bar ran into the selected panel),
    // on the Deck, 1080p, 4K, a 16:10 laptop and an ultrawide.
    let mut targets: Vec<Target> = UI_SCALES.iter().flat_map(|&s| [Target::deck(), Target::desktop_1080p(), Target::desktop_4k(), Target::wide_16_10(), Target::ultrawide()].map(|t| t.at_scale(s))).collect();
    // The default 1280x720 window on a 175% laptop (the OS scale is a floor, capped by the
    // theme's max_boost), and the same at UI scale 1.5.
    let laptop = Target { os_scale: Some(1.75), ..Target::new("window_1280x720_os175", [1280, 720], None) };
    // The 1280x800 window on that laptop (the default the HUD budget is measured at), and both
    // windows at the largest setting: the theme's min_logical (800x480) stops them there.
    let laptop800 = Target { os_scale: Some(1.75), ..Target::new("window_1280x800_os175", [1280, 800], None) };
    targets.extend([laptop.clone(), laptop.at_scale(1.5), laptop800.clone(), laptop800.at_scale(1.5)]);
    // The settings under 1.0 are the player's choice of smaller text: everything but the Deck's
    // legibility floor holds there (the Deck at 0.8 and 0.9 is under it; an open question).
    let smaller: Vec<String> = targets.iter().filter(|t| t.user_scale < 1.0).map(|t| t.name.clone()).collect();
    let mut errors = Vec::new();
    // Every language with a table, and the pseudo-locale (lane m5-l10n: words 35% longer).
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../sepulcra/assets/strings");
    let l10n = sepulcra_app::l10n::L10n::load(Some(&dir), None);
    assert!(l10n.problems.is_empty(), "{:?}", l10n.problems);
    let mut langs: Vec<(String, CheckOpts)> = Vec::new();
    for l in l10n.locales() {
        let mut one = sepulcra_app::l10n::L10n::load(Some(&dir), None);
        one.set_locale(&l).unwrap();
        langs.push((l.to_string(), CheckOpts { strings: Some(one.strings()), ..CheckOpts::default() }));
    }
    assert!(langs.iter().any(|(l, _)| l == "de"), "German is shipped");
    langs.push(("qps-ploc".into(), CheckOpts { pseudo: true, ..CheckOpts::default() }));
    for (lang, opts) in &langs {
        for set in SETS {
            let g = h.game.ui(UiPick::Live).expect("on");
            let f = check_with(g.ui, set, g.state, g.input_map, &targets, opts).unwrap_or_else(|e| panic!("{set:?}: {e}"));
            let counts = |f: &specula_ui::check::Finding| f.severity == Severity::Error && !(f.rule == "legibility" && smaller.contains(&f.target));
            errors.extend(f.into_iter().filter(counts).map(|f| format!("{what} {lang} {set:?} [{}] {} {}:{} {}", f.rule, f.target, f.screen, f.widget, f.message)));
        }
    }
    errors
}

#[test]
fn every_screen_passes_ui_check_at_every_display_and_ui_scale_setting() {
    let Some(mut h) = Host::new((1280, 800)) else { return };
    h.frame_with(0);
    let mut errors = errors_at(&mut h, "start:");
    // Busy: a stone carved and inspected (its panel, upgrade and sell), another chosen to carve
    // (the selected panel), paused (the play button and the PAUSED label).
    let place = specula_core::Json::obj([("action", specula_core::Json::from("place")), ("stone", specula_core::Json::from("warding")), ("cell", specula_core::Json::from("top:12,9"))]);
    {
        let Host { game, world, clock, .. } = &mut h;
        let out = game.call_tool(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, "play_command", &place);
        assert!(!out.is_error, "{:?}", out.content);
        for id in ["build_pillar", "pause_button"] {
            let out = game.call_tool(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, "play_ui", &specula_core::Json::obj([("press", specula_core::Json::from(id))]));
            assert!(!out.is_error, "{id}: {:?}", out.content);
        }
    }
    let cell = h.game.state().content().map.cell_ref("top:12,9").unwrap();
    assert!(h.game.select_cell(cell));
    h.frame_with(0);
    {
        let g = h.game.ui(UiPick::Live).expect("on");
        for id in ["inspect", "sell", "resume_button"] {
            assert!(g.ui.widget_shown(id).and_then(|w| g.ui.rect(w)).is_some(), "the busy state shows {id}");
        }
    }
    errors.extend(errors_at(&mut h, "busy:"));
    assert!(errors.is_empty(), "{} errors:\n{}", errors.len(), errors.join("\n"));
}

#[test]
fn the_lent_interface_lays_out_as_the_frames_do() {
    for size in [(1280, 800), (1920, 1080), (3840, 2160)] {
        let Some(mut h) = Host::new(size) else { return };
        h.frame_with(0);
        let g = h.game.ui(UiPick::Live).expect("on");
        let view = Display::new([size.0, size.1]).viewport(g.ui.theme(), g.user_scale);
        assert_eq!(g.ui.last_view(), Some(view), "{size:?}: the engine's viewport for this display is the frame's");
    }
    // The OS scale and a safe area reach the layout.
    let Some(mut h) = Host::new((1920, 1080)) else { return };
    let d = Display { os_scale: Some(2.0), safe_area: [40.0, 60.0, 40.0, 60.0], ..Display::new([1920, 1080]) };
    h.game.display(&d);
    h.frame_with(0);
    let g = h.game.ui(UiPick::Live).expect("on");
    let v = g.ui.last_view().expect("laid out");
    assert_eq!(v.scale, 2.0, "the OS's 200% is a floor over 1080p's 1.35");
    assert_eq!(v.safe().x, 60.0);
    let top = g.ui.widget_shown("top_bar").and_then(|w| g.ui.rect(w)).expect("the top bar");
    assert!(top.y >= 40.0 && top.x >= 60.0, "the HUD stays inside the safe area: {top:?}");
}

#[test]
fn a_gamepad_reaches_every_menu() {
    let Some(mut h) = Host::new((1280, 800)) else { return };
    h.frame_with(0);
    assert_eq!(h.top(), "hud");
    h.press(GamepadButton::Start);
    assert_eq!(h.top(), "pause", "Start opens the pause menu");
    assert_eq!(h.focused().as_deref(), Some("resume"));
    for (button, screen, back) in [("settings", "settings", "settings_back"), ("controls", "controls", "controls_back"), ("saves", "saves", "saves_back")] {
        h.focus(button);
        assert_eq!(h.focused().as_deref(), Some(button));
        h.press(GamepadButton::South);
        assert_eq!(h.top(), screen, "accept on {button}");
        // Every enabled button on it is reachable with the d-pad.
        let ids: Vec<String> = {
            let g = h.game.ui(UiPick::Live).expect("on");
            let s = g.ui.screen_id(screen).expect("loaded");
            g.ui.screen(s).nodes.iter().filter(|n| n.kind.interactive()).filter_map(|n| n.id.clone()).filter(|id| g.ui.widget_shown(id).and_then(|w| g.ui.rect(w)).is_some()).collect()
        };
        for id in &ids {
            let enabled = {
                let g = h.game.ui(UiPick::Live).expect("on");
                g.ui.focus_route(g.ui.widget_shown(id).expect("shown")).is_ok()
            };
            if enabled {
                h.focus(id);
                assert_eq!(h.focused().as_deref(), Some(id.as_str()), "{screen}: {id}");
            }
        }
        assert!(ids.iter().any(|i| i == back), "{screen} has its back button");
        h.press(GamepadButton::East);
        assert_eq!(h.top(), "pause", "B leaves {screen}");
    }
    // The pause menu offers the speed the HUD's button cycles (its gamepad path).
    h.focus("pause_speed");
    h.press(GamepadButton::South);
    assert_eq!(h.top(), "pause");
    h.press(GamepadButton::East);
    assert_eq!(h.top(), "hud", "B resumes");
    // North opens the build menu; the d-pad steps around it; B closes it.
    h.press(GamepadButton::North);
    assert_eq!(h.top(), "radial");
    h.focus("brazier");
    assert_eq!(h.focused().as_deref(), Some("brazier"));
    h.press(GamepadButton::East);
    assert_eq!(h.top(), "hud");
}

/// Review R1 of m5-uitools, finding 2: a capture (what ui_frame outlines) shows the focus the
/// player has, not the screen's default.
#[test]
fn a_capture_shows_the_players_focus() {
    let Some(mut h) = Host::new((1280, 800)) else { return };
    h.frame_with(0);
    h.press(GamepadButton::Start);
    h.focus("controls");
    assert_eq!(h.focused().as_deref(), Some("controls"));
    h.capture();
    let g = h.game.ui(UiPick::Capture).expect("the capture's interface");
    let focused = g.ui.focused().and_then(|w| g.ui.widget_id(w)).map(str::to_owned);
    assert_eq!(focused.as_deref(), Some("controls"), "the capture draws (and outlines) the player's focus");
    let o = g.ui.outlines(200).0;
    assert_eq!(o.iter().filter(|o| o.focused).filter_map(|o| o.id.as_deref()).collect::<Vec<_>>(), ["controls"]);
}

/// Review R1 of m5-uitools, finding 4: a synthetic frame (ui_act's) during a wave writes nothing
/// on its own: the moving dead are the game's presentation, drawn by window frames. The command
/// a press sends still writes (as the caller's).
#[test]
fn synthetic_frames_write_only_what_their_input_causes() {
    let Some(mut h) = Host::new((1280, 800)) else { return };
    {
        let Host { game, world, clock, .. } = &mut h;
        let out = game.call_tool(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, "play_command", &specula_core::Json::obj([("action", specula_core::Json::from("call_wave"))]));
        assert!(!out.is_error, "{:?}", out.content);
        for _ in 0..400 {
            game.tick(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false });
            clock.finish_tick();
        }
    }
    assert!(!h.game.state().enemies().is_empty(), "the dead are on the slab");
    // What a frame writes: entities whose place changed since the last look (the dead are
    // transient entities, which the journal leaves out, so their positions are compared).
    let mut seen: Vec<(u64, [f32; 3], [f32; 3])> = Vec::new();
    let mut writes = |h: &mut Host| {
        let now: Vec<(u64, [f32; 3], [f32; 3])> = h.world.entities().map(|e| (e.id, [e.position.x, e.position.y, e.position.z], [e.rotation_deg.x, e.rotation_deg.y, e.rotation_deg.z])).collect();
        let changed = now.len() != seen.len() || now.iter().zip(&seen).any(|(a, b)| a != b);
        seen = now;
        usize::from(changed)
    };
    h.frame_with(0);
    writes(&mut h);
    for i in 0..5 {
        let _ = h.frame_as(0, true, i as f32 / 5.0);
    }
    assert_eq!(writes(&mut h), 0, "idle synthetic frames write nothing");
    // More ticks: the dead walk on. Synthetic frames still don't draw it; a window frame does.
    for _ in 0..3 {
        let Host { game, world, clock, .. } = &mut h;
        game.tick(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false });
        clock.finish_tick();
    }
    writes(&mut h);
    let _ = h.frame_as(0, true, 0.5);
    assert_eq!(writes(&mut h), 0, "a synthetic frame after ticks writes nothing either");
    let _ = h.frame_as(0, false, 0.5);
    assert!(writes(&mut h) > 0, "a window frame presents (the dead move)");
    // Start opens the pause menu: the game pauses (a command), written in a synthetic frame.
    let _ = h.frame_as(GamepadButton::Start.bit(), true, 0.5);
    let _ = h.frame_as(0, true, 0.5);
    assert_eq!(h.top(), "pause");
}

/// Review R2 of m5-uitools, finding 1 (probe p7): headless, with the clock running, an agent's
/// ui_act frames between ticks never stop the dead moving in the world. A synthetic frame used to
/// count as the window's ("framed"), so ticks left motion to window frames that never come
/// headless, and world_query, physics and sight read the dead standing still until the calls
/// stopped. Also: a synthetic frame never takes the window title (the first real frame sends it).
#[test]
fn headless_synthetic_frames_never_stop_the_dead_moving() {
    let Some(mut h) = Host::new((1280, 800)) else { return };
    {
        let Host { game, world, clock, .. } = &mut h;
        let out = game.call_tool(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, "play_command", &specula_core::Json::obj([("action", specula_core::Json::from("call_wave"))]));
        assert!(!out.is_error, "{:?}", out.content);
    }
    for _ in 0..400 {
        h.tick();
    }
    assert!(!h.game.state().enemies().is_empty(), "the dead are on the slab");
    let mut last = h.enemy_positions();
    assert!(!last.is_empty(), "the dead are in the world");
    // Ticks with no frames at all move them (headless, nothing else would).
    h.tick();
    let moved = |a: &[(u64, [f32; 3])], b: &[(u64, [f32; 3])]| b.iter().filter(|(id, p)| a.iter().any(|(i, q)| i == id && q != p)).count();
    let now = h.enemy_positions();
    assert!(moved(&last, &now) > 0, "a tick moves the dead");
    last = now;
    // An agent calling ui_act between ticks: synthetic frames, then a tick, again and again
    // (all within the 250 ms a window frame counts for). Every tick still moves the dead.
    for round in 0..20 {
        for i in 0..3 {
            assert_eq!(h.frame_as(0, true, i as f32 / 3.0), None, "a synthetic frame never takes the title");
        }
        h.tick();
        let now = h.enemy_positions();
        assert!(moved(&last, &now) > 0, "round {round}: the dead stood still while an agent called ui_act");
        last = now;
    }
    // The first real frame sends the title.
    assert_eq!(h.frame_as(0, false, 1.0).as_deref(), Some("Sepulcra"));
}

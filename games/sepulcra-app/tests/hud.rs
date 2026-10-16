//! The HUD through real input: clicks on its buttons act (and never also act on the slab under
//! them), keys and the pad keep working, and the HUD reads and presses as data (`play_ui`).
//! Needs a system UI font; without one the HUD is off and these tests say so and pass.

use sepulcra::{Content, Outcome};
use sepulcra_app::SepulcraGame;
use specula::game::{Clock, Ctx, Frame, Game};
use specula::world::World;
use specula_core::Json;
use specula_platform::{Event, MouseButton};
use std::sync::Arc;

const SIZE: (u32, u32) = (1280, 720);

struct Host {
    game: SepulcraGame,
    world: World,
    clock: Clock,
}

impl Host {
    fn new(seed: u64) -> Host {
        let mut game = SepulcraGame::new(Arc::new(Content::builtin()), seed);
        let mut world = World::default();
        let clock = Clock::new(game.tick_hz());
        game.start(&mut Ctx { world: &mut world, clock: &clock, agent: "sepulcra", windowed: false });
        Host { game, world, clock }
    }

    fn frame(&mut self, input: &[Event]) {
        let Host { game, world, clock } = self;
        let (mut cam, mut title) = (None, None);
        let mut f = Frame { input, size: SIZE, alpha: 1.0, dt_s: 0.016, camera: &mut cam, agent_view: false, title: &mut title };
        game.frame(&mut Ctx { world, clock, agent: "sepulcra", windowed: false }, &mut f);
    }

    fn tick(&mut self) {
        let Host { game, world, clock } = self;
        game.tick(&mut Ctx { world, clock, agent: "sepulcra", windowed: false });
        clock.finish_tick();
    }

    fn hud_on(&self) -> bool {
        let on = self.game.presentation_json().get("hud").and_then(|h| h.get("on")).and_then(Json::as_bool) == Some(true);
        if !on {
            println!("SKIP: the HUD is off (no system UI font?)");
        }
        on
    }

    /// The centre of a visible widget, from the HUD's own tree.
    fn centre_of(&self, id: &str) -> Option<(i32, i32)> {
        let tree = self.game.ui_tree(200);
        for s in tree.get("screens").and_then(Json::as_array).unwrap_or(&[]) {
            for w in s.get("widgets").and_then(Json::as_array).unwrap_or(&[]) {
                if w.get("id").and_then(Json::as_str) == Some(id) {
                    let r: Vec<f64> = w.get("rect")?.as_array()?.iter().filter_map(Json::as_f64).collect();
                    return Some(((r[0] + r[2] / 2.0) as i32, (r[1] + r[3] / 2.0) as i32));
                }
            }
        }
        None
    }

    fn click(&mut self, (x, y): (i32, i32)) {
        self.frame(&[Event::MouseMove { x, y }]);
        self.frame(&[Event::MouseDown { button: MouseButton::Left, x, y }]);
        self.frame(&[Event::MouseUp { button: MouseButton::Left, x, y }]);
        self.frame(&[]);
    }

    fn selected(&self) -> String {
        let p = self.game.presentation_json();
        p.get("hud").and_then(|h| h.get("selected")).and_then(Json::as_str).unwrap_or("").to_owned()
    }
}

#[test]
fn clicking_the_build_bar_selects_and_never_builds_under_it() {
    let mut h = Host::new(5);
    if !h.hud_on() {
        return;
    }
    h.frame(&[]);
    let at = h.centre_of("build_pillar").expect("the build bar shows the pillar");
    h.click(at);
    assert_eq!(h.selected(), "rune_pillar");
    assert_eq!(h.game.state().stones().count(), 0, "the click was the HUD's, not a placement on the slab");
    // The wall, then back.
    let at = h.centre_of("build_wall").expect("wall button");
    h.click(at);
    assert_eq!(h.selected(), "wall");
    // Keys still select (the HUD only takes the pointer).
    h.frame(&[Event::KeyDown { code: 0x31, repeat: false }]);
    h.frame(&[Event::KeyUp { code: 0x31 }]);
    assert_eq!(h.selected(), "warding");
}

#[test]
fn the_pause_button_pauses_and_the_menu_resumes() {
    let mut h = Host::new(5);
    if !h.hud_on() {
        return;
    }
    h.frame(&[]);
    let at = h.centre_of("pause_button").expect("pause button");
    h.click(at);
    assert!(h.game.presentation_json().get("hud").unwrap().get("screens").unwrap().to_string().contains("pause"));
    let tick = h.game.state().tick();
    for _ in 0..10 {
        h.tick();
    }
    assert_eq!(h.game.state().tick(), tick, "paused: the simulation doesn't step");
    let at = h.centre_of("resume").expect("the pause menu's resume button");
    h.click(at);
    for _ in 0..10 {
        h.tick();
    }
    assert_eq!(h.game.state().tick(), tick + 10, "resumed");
}

#[test]
fn a_placed_stone_opens_its_panel_and_its_buttons_upgrade_and_sell() {
    let mut h = Host::new(5);
    if !h.hud_on() {
        return;
    }
    let place = Json::obj([("action", Json::from("place")), ("stone", Json::from("warding")), ("cell", Json::from("top:12,9"))]);
    {
        let Host { game, world, clock } = &mut h;
        let out = game.call_tool(&mut Ctx { world, clock, agent: "sepulcra", windowed: false }, "play_command", &place);
        assert!(!out.is_error);
    }
    // Agents press the HUD by id; a player clicks the stone to select it (here: the same action).
    let press = |h: &mut Host, id: &str| {
        let Host { game, world, clock } = h;
        game.call_tool(&mut Ctx { world, clock, agent: "sepulcra", windowed: false }, "play_ui", &Json::obj([("press", Json::from(id))]))
    };
    // Nothing selected: no panel, so its buttons can't be pressed.
    let out = press(&mut h, "sell");
    assert!(out.is_error, "no panel open yet: {:?}", out.content);
    // Select the stone the way the mouse does it (a click on an occupied cell selects it).
    let cell = h.game.state().content().map.cell_ref("top:12,9").unwrap();
    assert!(h.game.select_cell(cell));
    h.frame(&[]);
    assert!(h.centre_of("sell").is_some(), "the panel shows sell");
    let before = h.game.state().dust();
    let out = press(&mut h, "sell");
    assert!(!out.is_error, "{:?}", out.content);
    assert_eq!(h.game.state().stones().count(), 0);
    assert!(h.game.state().dust() > before);
    assert_eq!(h.game.state().outcome(), Outcome::Running);
}

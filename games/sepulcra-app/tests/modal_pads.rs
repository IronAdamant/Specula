//! The engine's review screen is modal for gamepads too (M4 review lane, integrator's follow-up):
//! Sepulcra reads its pads from the engine's frame (`Frame::pads`), which the engine releases
//! while the screen holds the input ([`PadGate`]). A pad press then changes nothing in
//! `play_state`; the same press without the screen calls the next wave.

use sepulcra::Content;
use sepulcra_app::SepulcraGame;
use specula::game::{Clock, Ctx, Frame, Game, GameWorld, PadGate};
use specula::world::World;
use specula_core::Json;
use specula_platform::{GamepadButton, GamepadState};
use std::sync::Arc;

fn text(out: &specula_protocol::ToolOutput) -> String {
    match &out.content[0] {
        specula_protocol::Content::Text(t) => t.clone(),
        _ => String::new(),
    }
}

struct Host {
    game: SepulcraGame,
    world: World,
    clock: Clock,
    gate: PadGate,
}

impl Host {
    fn new() -> Host {
        let mut game = SepulcraGame::new(Arc::new(Content::builtin()), 7);
        let mut world = World::default();
        let clock = Clock::new(game.tick_hz());
        game.start(&mut Ctx { world: GameWorld::new(&mut world), clock: &clock, agent: "sepulcra", windowed: false });
        let mut h = Host { game, world, clock, gate: PadGate::default() };
        h.call("play_new", Json::obj([("seed", Json::from("7")), ("layout", Json::from("centre"))]));
        h
    }

    fn call(&mut self, name: &str, args: Json) -> String {
        let Host { game, world, clock, .. } = self;
        text(&game.call_tool(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, name, &args))
    }

    /// One window frame with the left stick clicked (call_wave) or not, the review screen open
    /// (`captured`) or not, then one tick.
    fn step(&mut self, l3: bool, captured: bool) {
        let b = if l3 { GamepadButton::LeftStick.bit() } else { 0 };
        let mut pad = GamepadState { connected: true, buttons: b, ..GamepadState::default() };
        pad.raw.buttons = b;
        let mut pads = [pad];
        self.gate.apply(captured, &mut pads);
        let Host { game, world, clock, .. } = self;
        let (mut cam, mut title) = (None, None);
        let mut f = Frame { input: &[], size: (1280, 720), alpha: 1.0, dt_s: 0.016, camera: &mut cam, agent_view: false, title: &mut title, pads: &pads, synthetic: false };
        game.frame(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false }, &mut f);
        game.tick(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false });
        self.clock.finish_tick();
    }

    fn state(&mut self) -> String {
        self.call("play_state", Json::obj::<&str>([]))
    }
}

#[test]
fn a_pad_press_on_the_open_review_screen_changes_nothing_in_play() {
    // `pressed` drives the pad; `control` gets the same frames without any press.
    let (mut pressed, mut control) = (Host::new(), Host::new());
    // The screen is open: L3 pressed, held, then the screen closes while it is still held.
    for (l3, captured) in [(false, true), (true, true), (true, true), (true, false), (true, false)] {
        pressed.step(l3, captured);
        control.step(false, captured);
    }
    assert_eq!(pressed.state(), control.state(), "presses made on the review screen never reach the game");
    // Let go, then pressed with the screen closed: the game calls the next wave.
    pressed.step(false, false);
    control.step(false, false);
    pressed.step(true, false);
    control.step(false, false);
    assert_ne!(pressed.state(), control.state(), "the same press without the screen acts in the game");
}

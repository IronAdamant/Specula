//! Regression tests from the lane review (2026-10-13): flashes in window frames, minimized
//! windows, dry runs, and recording paths.

use sepulcra::{Content, Outcome};
use sepulcra_app::controls::Controls;
use sepulcra_app::slab::SlabGeo;
use sepulcra_app::SepulcraGame;
use specula::game::{Clock, Ctx, Frame, Game};
use specula::world::World;
use specula_core::Json;
use specula_platform::GamepadState;
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
}

impl Host {
    fn new(seed: u64) -> Host {
        let mut game = SepulcraGame::new(Arc::new(Content::builtin()), seed);
        let mut world = World::default();
        let clock = Clock::new(game.tick_hz());
        game.start(&mut Ctx { world: &mut world, clock: &clock, agent: "sepulcra", windowed: false });
        Host { game, world, clock }
    }

    fn call(&mut self, name: &str, args: &Json) -> specula_protocol::ToolOutput {
        let Host { game, world, clock } = self;
        game.call_tool(&mut Ctx { world, clock, agent: "sepulcra", windowed: false }, name, args)
    }

    fn tick(&mut self) {
        let Host { game, world, clock } = self;
        game.tick(&mut Ctx { world, clock, agent: "sepulcra", windowed: false });
        self.clock.finish_tick();
    }

    fn frame(&mut self, alpha: f32, size: (u32, u32)) {
        let Host { game, world, clock } = self;
        let (mut cam, mut title) = (None, None);
        let mut f = Frame { input: &[], size, alpha, dt_s: 0.016, camera: &mut cam, title: &mut title };
        game.frame(&mut Ctx { world, clock, agent: "sepulcra", windowed: false }, &mut f);
    }

    fn active_flashes(&self) -> usize {
        let Ok(fx) = self.world.find_path("/fx") else { return 0 };
        self.world.children_of(Some(fx)).filter(|&id| self.world.get(id).unwrap().position.y > -8.0).count()
    }

    /// Positions of the enemies on the slab (not parked in the pool).
    fn enemy_positions(&self) -> Vec<(u64, [f32; 3])> {
        let Ok(g) = self.world.find_path("/units/enemies") else { return Vec::new() };
        self.world
            .children_of(Some(g))
            .filter_map(|id| {
                let e = self.world.get(id).unwrap();
                (e.position.y > -8.5).then_some((id, [e.position.x, e.position.y, e.position.z]))
            })
            .collect()
    }
}

#[test]
fn tick_born_flashes_survive_the_next_window_frame() {
    let mut h = Host::new(7);
    h.call("play_new", &Json::obj([("seed", Json::from("7")), ("layout", Json::from("centre"))]));
    let (mut after_tick, mut after_frame, mut kept) = (0, 0, 0);
    for _ in 0..6000 {
        if h.game.state().outcome() != Outcome::Running {
            break;
        }
        h.tick();
        let a = h.active_flashes();
        h.frame(0.3, (1280, 720));
        let b = h.active_flashes();
        after_tick = after_tick.max(a);
        after_frame = after_frame.max(b);
        if a > 0 && b >= a {
            kept += 1;
        }
    }
    assert!(after_frame > 0, "flashes born in tick() were parked by the next frame (after tick: {after_tick})");
    assert!(kept > 50, "a frame right after a tick keeps that tick's flashes ({kept} ticks)");
}

#[test]
fn a_paused_engine_clock_draws_the_newest_tick() {
    let mut h = Host::new(7);
    h.call("play_new", &Json::obj([("seed", Json::from("7")), ("layout", Json::from("centre"))]));
    h.clock.set_paused(true);
    // Agents step a paused clock: each tick leaves everything where it is at that tick.
    for _ in 0..1200 {
        h.tick();
    }
    let at_tick = h.enemy_positions();
    assert!(!at_tick.is_empty(), "wave 1 is on the slab by now");
    // A window frame while the clock is paused (alpha 0) must not pull things back a tick.
    h.frame(0.0, (1280, 720));
    assert_eq!(h.enemy_positions(), at_tick);
}

#[test]
fn a_minimized_window_does_not_panic() {
    let content = Content::builtin();
    let geo = SlabGeo::new(&content.map);
    let mut c = Controls::new();
    let pad = GamepadState { connected: true, left_stick: [1.0, 0.0], ..GamepadState::default() };
    for i in 0..3 {
        c.frame_at(0.016 * i as f64, &[], &[pad], 0.016, (0, 0), &geo, &content.map);
    }
    // Through the game too (the engine calls frame before its minimized check).
    let mut h = Host::new(3);
    h.tick();
    h.frame(0.5, (0, 0));
}

#[test]
fn single_command_dry_run_is_accepted() {
    let mut h = Host::new(11);
    let out = h.call("play_command", &Json::obj([("action", Json::from("call_wave")), ("dry_run", Json::Bool(true))]));
    assert!(!out.is_error, "{}", text(&out));
    assert_eq!(h.game.state().wave(), 0, "a dry run changes nothing");
}

#[test]
fn a_dry_run_batch_matches_the_real_batch() {
    let mut h = Host::new(11);
    let batch = Json::arr([
        Json::obj([("action", Json::from("place")), ("stone", Json::from("warding")), ("cell", Json::from("top:12,9"))]),
        Json::obj([("action", Json::from("upgrade")), ("cell", Json::from("top:12,9"))]),
        Json::obj([("action", Json::from("place")), ("stone", Json::from("brazier")), ("cell", Json::from("top:7,7"))]),
    ]);
    let dry = h.call("play_command", &Json::obj([("commands", batch.clone()), ("dry_run", Json::Bool(true))]));
    let hash = h.game.state().hash();
    let real = h.call("play_command", &Json::obj([("commands", batch)]));
    let (d, r) = (Json::parse(&text(&dry)).unwrap(), Json::parse(&text(&real)).unwrap());
    assert_eq!(d.get("accepted"), r.get("accepted"));
    assert_eq!(d.get("rejected"), r.get("rejected"));
    assert_eq!(d.get("accepted").and_then(Json::as_u64), Some(2), "place + upgrade fit, the brazier doesn't: {d}");
    assert_ne!(hash, h.game.state().hash(), "the real batch changed the state");
}

#[test]
fn recordings_refuse_device_names_and_streams() {
    let mut h = Host::new(3);
    for p in ["NUL", "sub/CON.json", "rec.json:stream", "CONIN$", "lpt1.txt", "nul .json", "aux."] {
        let out = h.call("play_record", &Json::obj([("path", Json::from(p))]));
        assert!(out.is_error && text(&out).contains("bad_path"), "{p}: {}", text(&out));
    }
}

#[cfg(windows)]
#[test]
fn recordings_do_not_follow_junctions_out_of_the_working_directory() {
    let outside = std::env::temp_dir().join(format!("sepulcra-app-review-outside-{}", std::process::id()));
    std::fs::create_dir_all(&outside).unwrap();
    let link = format!("review-junction-{}", std::process::id());
    let made = std::process::Command::new("cmd").args(["/C", "mklink", "/J", &link, outside.to_str().unwrap()]).output();
    if !made.as_ref().is_ok_and(|o| o.status.success()) {
        eprintln!("skipped: could not create a junction ({made:?})");
        return;
    }
    let mut h = Host::new(3);
    let out = h.call("play_record", &Json::obj([("path", Json::from(format!("{link}/escaped.json")))]));
    let escaped = outside.join("escaped.json").exists();
    let _ = std::fs::remove_dir(&link);
    let _ = std::fs::remove_dir_all(&outside);
    assert!(!escaped, "wrote outside the working directory");
    assert!(out.is_error && text(&out).contains("bad_path"), "{}", text(&out));
}

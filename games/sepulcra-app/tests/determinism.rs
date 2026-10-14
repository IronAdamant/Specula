//! The hosted game advances exactly like the headless runner (foundation F2): the same seed and
//! the same commands at the same ticks give the same state hash on every tick. Driven through
//! the engine's `Game` trait with a plain `World` (no GPU, no window).

use sepulcra::script::{command_to_json, replay, Script};
use sepulcra::{Content, Outcome};
use sepulcra_app::SepulcraGame;
use specula::game::{Clock, Ctx, Game};
use specula::world::World;
use specula_core::Json;
use std::sync::Arc;

const SEED: u64 = 7;

fn centre(content: &Content) -> Script {
    let text = sepulcra_app::tools::LAYOUTS.iter().find(|(n, _)| *n == "centre").unwrap().1;
    Script::parse(text, content, "centre.json").unwrap()
}

/// `{"place": "warding", "at": "top:8,12"}` → play_command's `{"action", "stone", "cell"}`.
fn tool_args(cmd: &Json) -> Json {
    let pairs = cmd.as_object().unwrap();
    let (action, value) = pairs.iter().find(|(k, _)| k != "at" && k != "tick").unwrap();
    let mut args = Json::obj([("action", Json::from(action.as_str()))]);
    match action.as_str() {
        "place" => {
            args.set("stone", value.clone());
            args.set("cell", cmd.get("at").unwrap().clone());
        }
        "call_wave" => {}
        _ => {
            args.set("cell", value.clone());
        }
    }
    args
}

fn text(out: &specula_protocol::ToolOutput) -> String {
    match &out.content[0] {
        specula_protocol::Content::Text(t) => t.clone(),
        _ => panic!("expected text"),
    }
}

struct Host {
    game: SepulcraGame,
    world: World,
    clock: Clock,
}

impl Host {
    fn new(content: Arc<Content>, seed: u64) -> Host {
        let mut game = SepulcraGame::new(content, seed);
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
}

#[test]
fn commands_through_the_game_match_the_headless_run_tick_by_tick() {
    let content = Arc::new(Content::builtin());
    let report = sepulcra::run(Arc::clone(&content), SEED, &centre(&content), None, |_, _| {});
    assert_eq!(report.outcome, Outcome::Won, "the reference run should be a win");
    let mut hashes = Vec::new();
    replay(Arc::clone(&content), SEED, &report.journal, u32::MAX, |s, _| hashes.push(s.hash()));
    assert_eq!(hashes.len() as u32, report.ticks);

    let mut host = Host::new(Arc::clone(&content), SEED);
    assert_eq!(host.game.tick_hz(), sepulcra::TICK_HZ);
    let mut k = 0;
    while host.game.state().outcome() == Outcome::Running {
        let tick = host.game.state().tick();
        while k < report.journal.len() && report.journal[k].0 == tick {
            let args = tool_args(&command_to_json(&report.journal[k].1, &content));
            let out = host.call("play_command", &args);
            assert!(!out.is_error, "tick {tick}: {}", text(&out));
            k += 1;
        }
        host.tick();
        let t = host.game.state().tick();
        assert_eq!(host.game.state().hash(), hashes[t as usize - 1], "diverged at tick {t}");
    }
    assert_eq!(k, report.journal.len());
    assert_eq!(host.game.state().tick(), report.ticks);
    let state = Json::parse(&text(&host.call("play_state", &Json::Obj(Vec::new())))).unwrap();
    assert_eq!(state.get("state_hash").and_then(Json::as_str), Some(format!("{:016x}", report.hash).as_str()));
    assert_eq!(state.get("outcome").and_then(Json::as_str), Some("won"));
    assert_eq!(host.game.journal(), &report.journal, "the game records what it applied");
    // The presentation followed along: one stone group per stone.
    let stones = host.world.find_path("/units/stones").unwrap();
    assert_eq!(host.world.children_of(Some(stones)).count(), host.game.state().stones().count());
}

#[test]
fn a_layout_followed_by_the_game_ends_like_the_headless_run() {
    let content = Arc::new(Content::builtin());
    let report = sepulcra::run(Arc::clone(&content), SEED, &centre(&content), None, |_, _| {});
    let mut host = Host::new(Arc::clone(&content), 1);
    let out = host.call("play_new", &Json::obj([("seed", Json::from("7")), ("layout", Json::from("centre"))]));
    assert!(!out.is_error, "{}", text(&out));
    while host.game.state().outcome() == Outcome::Running {
        host.tick();
    }
    assert_eq!((host.game.state().tick(), host.game.state().hash()), (report.ticks, report.hash));
}

#[test]
fn pausing_and_fast_forward_do_not_change_the_run() {
    let content = Arc::new(Content::builtin());
    let report = sepulcra::run(Arc::clone(&content), SEED, &centre(&content), Some(3), |_, _| {});
    let mut host = Host::new(Arc::clone(&content), 1);
    host.call("play_new", &Json::obj([("seed", Json::from(7)), ("layout", Json::from("centre"))]));
    let mut n = 0;
    while host.game.state().tick() < report.ticks {
        // Alternate: paused for a while, then double and quadruple speed.
        let control = match (n / 50) % 3 {
            0 => Json::obj([("paused", Json::Bool(true))]),
            1 => Json::obj([("paused", Json::Bool(false)), ("speed", Json::from(2))]),
            _ => Json::obj([("speed", Json::from(4))]),
        };
        host.call("play_control", &control);
        host.tick();
        n += 1;
        // Fast speed may overshoot by a few ticks; compare at the exact tick below.
        if host.game.state().tick() + 4 > report.ticks {
            host.call("play_control", &Json::obj([("speed", Json::from(1)), ("paused", Json::Bool(false))]));
        }
    }
    assert_eq!(host.game.state().tick(), report.ticks);
    assert_eq!(host.game.state().hash(), report.hash);
}

#[test]
fn recordings_verify_even_right_after_a_command() {
    let content = Arc::new(Content::builtin());
    let mut host = Host::new(Arc::clone(&content), 11);
    let place = Json::obj([("action", Json::from("place")), ("stone", Json::from("warding")), ("cell", Json::from("top:12,9"))]);
    for _ in 0..40 {
        host.tick();
    }
    assert!(!host.call("play_command", &place).is_error);
    // Rejections are structured, with the simulation's code.
    let again = host.call("play_command", &place);
    assert!(again.is_error);
    let err = Json::parse(&text(&again)).unwrap();
    assert_eq!(err.get("error").and_then(Json::as_str), Some("rejected"));
    assert_eq!(err.get("code").and_then(Json::as_str), Some("occupied"));
    let brazier = Json::obj([("action", Json::from("place")), ("stone", Json::from("brazier")), ("cell", Json::from("top:7,7"))]);
    assert!(!host.call("play_command", &brazier).is_error, "150 - 50 leaves enough for a brazier");
    let poor = host.call("play_command", &Json::obj([("action", Json::from("place")), ("stone", Json::from("sky_obelisk")), ("cell", Json::from("top:11,7"))]));
    let err = Json::parse(&text(&poor)).unwrap();
    assert_eq!(err.get("code").and_then(Json::as_str), Some("not_enough_dust"), "{err}");
    assert!(err.get("need").is_some() && err.get("have").is_some());

    let dir = "sepulcra-app-test-recordings";
    let path = format!("{dir}/rec.json");
    let rec = host.call("play_record", &Json::obj([("path", Json::from(path.as_str()))]));
    assert!(!rec.is_error, "{}", text(&rec));
    let rep = host.call("play_replay", &Json::obj([("path", Json::from(path.as_str()))]));
    let _ = std::fs::remove_dir_all(dir);
    assert!(!rep.is_error, "{}", text(&rep));
    let rep = Json::parse(&text(&rep)).unwrap();
    assert_eq!(rep.get("status").and_then(Json::as_str), Some("verified"));
    // Paths must stay inside the working directory.
    let bad = host.call("play_record", &Json::obj([("path", Json::from("../escape.json"))]));
    assert!(bad.is_error && text(&bad).contains("bad_path"));
}

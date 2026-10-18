//! Save games (docs/design/SAVES.md) through the engine's `Game` trait with a plain `World` (no
//! GPU, no window): a run saved mid-wave and loaded into another game instance continues with
//! the same world hash and the same simulation subsystem hashes on every later tick, adopting
//! the stones, walls and tombstones the saved world holds instead of rebuilding them.

use sepulcra::Content;
use sepulcra_app::SepulcraGame;
use specula::game::{Clock, Ctx, Game};
use specula::world::World;
use specula_core::Json;
use std::sync::Arc;

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

    fn call(&mut self, name: &str, args: &str) {
        let Host { game, world, clock } = self;
        let out = game.call_tool(&mut Ctx { world, clock, agent: "sepulcra", windowed: false }, name, &Json::parse(args).unwrap());
        assert!(!out.is_error, "{name} {args}: {:?}", out.content);
    }

    fn tick(&mut self) {
        let Host { game, world, clock } = self;
        game.tick(&mut Ctx { world, clock, agent: "sepulcra", windowed: false });
        self.clock.finish_tick();
    }

    fn hashes(&self) -> (u64, Vec<(&'static str, u64)>) {
        let mut subs = Vec::new();
        self.game.state_hashes(&mut subs);
        (self.world.state_hash(), subs)
    }
}

fn load(files: &[(String, String)]) -> World {
    specula::persist::load_with(|f| {
        files.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| specula::persist::PersistError::new("not_found", f.to_owned()))
    })
    .unwrap()
    .0
}

#[test]
fn a_loaded_run_continues_tick_for_tick() {
    let content = Arc::new(Content::builtin());
    let mut a = Host::new(Arc::clone(&content), 7);
    a.call("play_new", r#"{"seed": 7, "layout": "centre"}"#);
    for _ in 0..2400 {
        a.tick();
    }
    // A player's command between ticks, then a speed change, so the save holds both.
    a.call("play_command", r#"{"action": "call_wave"}"#);
    a.call("play_control", r#"{"speed": 2}"#);
    let bytes = a.game.save_state().expect("Sepulcra saves").unwrap();
    let files = specula::persist::render(&a.world);
    let tick = a.clock.tick();

    // Another game instance (another seed, its own world), as an engine at start-up.
    let mut b = Host::new(Arc::clone(&content), 99);
    b.world = load(&files);
    b.clock.set_tick(tick);
    let parsed = b.game.parse_state(&bytes).unwrap();
    let Host { game, world, clock } = &mut b;
    game.apply_state(&mut Ctx { world, clock, agent: "sepulcra", windowed: false }, parsed);
    assert_eq!(b.hashes(), a.hashes(), "the load adopted the world without changing it");
    assert!(b.world.transient_len() > 0, "the transient presentation was rebuilt");
    for t in 0..3000 {
        a.tick();
        b.tick();
        assert_eq!(b.hashes(), a.hashes(), "diverged {t} ticks after the load");
    }
    assert_eq!(b.game.state().hash(), a.game.state().hash());
    assert_eq!(b.game.journal(), a.game.journal(), "the run's recording carried over");
}

/// The review's fuzzer found a one-bit change (an enemy's movement remainder set to `i64::MIN`)
/// that a load accepted and the next tick panicked on. Loading it now refuses the game section;
/// whatever does load must step without panicking.
#[test]
fn the_fuzzers_crash_file_is_refused_not_a_crash() {
    let bytes = include_bytes!("data/crash_1_57.specsave");
    let file = specula::savegame::decode(bytes).expect("the container itself is intact");
    let content = Arc::new(Content::builtin());
    let mut b = Host::new(Arc::clone(&content), 7);
    let tick = file.meta.get("tick").and_then(Json::as_u64).unwrap();
    let parsed = b.game.parse_state(file.section("game").unwrap());
    let files = specula::savegame::unpack_files(file.section("world").unwrap()).unwrap();
    match parsed {
        Err(e) => assert!(e.contains("bad_state"), "{e}"),
        Ok(state) => {
            b.world = load(&files);
            b.clock.set_tick(tick);
            let Host { game, world, clock } = &mut b;
            game.apply_state(&mut Ctx { world, clock, agent: "sepulcra", windowed: false }, state);
            for _ in 0..3000 {
                b.tick();
            }
        }
    }
}

#[test]
fn saves_from_other_content_or_damaged_ones_are_refused_before_anything_changes() {
    let content = Arc::new(Content::builtin());
    let mut a = Host::new(Arc::clone(&content), 3);
    for _ in 0..300 {
        a.tick();
    }
    let bytes = a.game.save_state().unwrap().unwrap();
    let b = Host::new(Arc::clone(&content), 3);
    for cut in (0..bytes.len()).step_by(11) {
        assert!(b.game.parse_state(&bytes[..cut]).is_err(), "cut at {cut}");
    }
    let mut other = Content::builtin();
    other.hash ^= 0x55;
    let c = Host::new(Arc::new(other), 3);
    let Err(e) = c.game.parse_state(&bytes) else { panic!("other content must be refused") };
    assert!(e.contains("content_changed"), "{e}");
}

/// The Save requests the game made since the last call (slot names), dropping its list refreshes.
fn saves_asked(host: &mut Host) -> Vec<String> {
    let mut out = Vec::new();
    host.game.take_requests(&mut out);
    out.into_iter().filter_map(|r| if let specula::game::GameRequest::Save { slot, .. } = r { Some(slot) } else { None }).collect()
}

#[test]
fn wave_starts_autosave_in_turn_when_switched_on() {
    let content = Arc::new(Content::builtin());
    let mut h = Host::new(content, 7);
    h.call("play_new", r#"{"seed": 7, "layout": "centre"}"#);
    // Headless (agents' runs): off by default.
    h.call("play_command", r#"{"action": "call_wave"}"#);
    h.tick();
    assert_eq!(saves_asked(&mut h), Vec::<String>::new(), "no autosave headless unless switched on");
    // Settings switches it on (the same button a player presses).
    h.call("play_ui", r#"{"press": "menu_button"}"#);
    h.call("play_ui", r#"{"press": "settings"}"#);
    h.call("play_ui", r#"{"press": "autosave"}"#);
    h.call("play_ui", r#"{"press": "settings_back"}"#);
    h.call("play_ui", r#"{"press": "resume"}"#);
    saves_asked(&mut h);
    // A wave called early (a command) and the next starting on its own (a tick), in turn.
    h.call("play_command", r#"{"action": "call_wave"}"#);
    assert_eq!(saves_asked(&mut h), ["sepulcra_autosave_2"], "wave 2 (called early) saves to autosave 2");
    let mut asked = Vec::new();
    for _ in 0..60 * 60 * 5 {
        h.tick();
        asked.extend(saves_asked(&mut h));
        if !asked.is_empty() {
            break;
        }
    }
    assert_eq!(asked, ["sepulcra_autosave_1"], "wave 3 (started by the clock) saves to autosave 1");
}

/// Loads parse the game's section on their worker thread (`Game::state_parser`), not on the
/// engine thread: the parser is `Send`, gives what `parse_state` gives, and refuses what it
/// refuses. The review's crafted save (a 32 MB recording inside a 1 MB file) is refused by the
/// recording cap, on the worker.
#[test]
fn the_game_section_is_parsed_off_the_engine_thread() {
    let content = Arc::new(Content::builtin());
    let mut a = Host::new(Arc::clone(&content), 7);
    a.call("play_new", r#"{"seed": 7, "layout": "centre"}"#);
    for _ in 0..1200 {
        a.tick();
    }
    let bytes = a.game.save_state().unwrap().unwrap();
    let b = Host::new(Arc::clone(&content), 7);
    let parse = b.game.state_parser().expect("Sepulcra parses saves on the load's worker");
    let data = bytes.clone();
    let parsed = std::thread::spawn(move || parse(&data).map(|_| ())).join().unwrap();
    assert!(parsed.is_ok(), "{parsed:?}");
    // The game section with a recording far over the cap (as a crafted file would carry it).
    let state_len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let at = 8 + state_len;
    let rec_len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let mut big = Vec::from(&bytes[..at]);
    let rec = format!("{{\"x\": [{}0]}}", "0,".repeat(1_600_000));
    assert!(rec.len() > sepulcra_app::game::MAX_RECORDING_BYTES);
    big.extend_from_slice(&(rec.len() as u32).to_le_bytes());
    big.extend_from_slice(rec.as_bytes());
    big.extend_from_slice(&bytes[at + 4 + rec_len..]);
    let parse = b.game.state_parser().unwrap();
    let started = std::time::Instant::now();
    let Err(e) = std::thread::spawn(move || parse(&big).map(|_| ())).join().unwrap() else { panic!("an oversized recording must be refused") };
    assert!(e.contains("recording") || e.contains("damaged"), "{e}");
    assert!(started.elapsed().as_millis() < 500, "refused by the cap before parsing: {:?}", started.elapsed());
}

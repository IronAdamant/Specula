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

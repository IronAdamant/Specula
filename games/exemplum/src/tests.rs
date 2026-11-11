//! The demo played end to end with all six modules linked into the engine's real module host:
//! the four gameplay modules talk to each other only through the world and events, and the run
//! is deterministic (twin runs match) and survives a save and load mid-quest.

use super::*;
use specula::logic::tools::no_journal;
use specula::logic::LogicHost;
use specula::world::{EntityKey, UpdateSpec, World};
use specula_core::Json;
use specula_logic::ModuleCode;

const HZ: u32 = 30;

struct Run {
    host: LogicHost,
    world: World,
    tick: u64,
}

impl Run {
    fn new() -> Run {
        let mut r = Run { host: LogicHost::new(), world: World::default(), tick: 0 };
        for code in [
            ModuleCode::of::<ambulo::Ambulo>(),
            ModuleCode::of::<inventarium::Inventarium>(),
            ModuleCode::of::<facultas::Facultas>(),
            ModuleCode::of::<colloquium::Colloquium>(),
            ModuleCode::of::<quaestio::Quaestio>(),
            ModuleCode::of::<Exemplum>(),
        ] {
            r.host.load(code, Default::default(), Some(11), &mut r.world, 0, HZ, &mut no_journal()).expect("loads");
        }
        r.ticks(2);
        r
    }

    fn ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.host.tick(&mut self.world, self.tick, HZ, &mut no_journal());
            self.tick += 1;
        }
    }

    fn press(&mut self, module: &str, action: &str) {
        self.host.hold(module, &Json::parse(&format!(r#"{{"{action}": true}}"#)).unwrap(), 1).unwrap();
        self.ticks(1);
        self.host.hold(module, &Json::parse(&format!(r#"{{"{action}": false}}"#)).unwrap(), 1).unwrap();
        self.ticks(3);
    }

    fn go(&mut self, x: f32, z: f32) {
        let to = UpdateSpec { position: Some(specula_core::math::Vec3::new(x, 0.0, z)), ..Default::default() };
        self.world.update(&EntityKey::Path("/ambulo/player".into()), to, "tester").unwrap();
        self.ticks(2);
    }

    fn player(&self) -> &specula::world::Entity {
        self.world.get(self.world.find_path("/ambulo/player").unwrap()).unwrap()
    }

    fn items(&self, item: &str) -> u64 {
        self.player().props.get("inventory").and_then(|i| i.get("slots")).and_then(Json::as_array).unwrap_or_default().iter().filter(|s| s.get("item").and_then(Json::as_str) == Some(item)).filter_map(|s| s.get("count").and_then(Json::as_u64)).sum()
    }

    fn quest(&self) -> Json {
        self.player().props.get("quests").and_then(|q| q.get("bones")).cloned().unwrap_or(Json::Null)
    }

    fn node(&self) -> Option<String> {
        self.player().props.get("colloquium").and_then(|c| c.get("node")).and_then(Json::as_str).map(str::to_owned)
    }

    fn hashes(&mut self) -> (u64, Option<u64>) {
        (self.world.state_hash(), self.host.state_hash())
    }

    fn running(&self) {
        for m in ["ambulo", "inventarium", "facultas", "colloquium", "quaestio", "exemplum"] {
            assert!(self.host.running(m), "{m} is still running: {}", self.host.status(Some(m), true).unwrap());
        }
    }
}

/// Part 1: take the quest, get the lantern, pick up three bones.
fn first_half(r: &mut Run) {
    r.go(2.2, 2.0);
    r.press("colloquium", "talk");
    assert_eq!(r.node().as_deref(), Some("greet"));
    r.press("colloquium", "choice_1");
    assert_eq!(r.node().as_deref(), Some("offer"));
    r.press("colloquium", "choice_1");
    assert_eq!(r.node().as_deref(), Some("accepted"));
    r.press("colloquium", "talk");
    assert_eq!(r.node(), None, "the talk ended");
    assert_eq!(r.quest().get("state").and_then(Json::as_str), Some("active"));
    assert_eq!(r.items("lantern"), 1, "the Sexton's lantern");
    for (x, z) in [(-1.8, -3.0), (0.2, -3.2), (-3.6, -1.6)] {
        r.go(x, z);
        r.press("inventarium", "interact");
    }
    assert_eq!(r.items("bone"), 3);
    assert_eq!(r.quest().get("progress").and_then(|p| p.get("collect")).and_then(Json::as_u64), Some(3));
}

/// Part 2: banish two wisps with the lantern flare, hand the bones over, collect the rewards.
fn second_half(r: &mut Run) {
    r.go(-0.5, -6.0);
    r.press("facultas", "ability_1");
    r.ticks(80);
    r.press("facultas", "ability_1");
    r.ticks(4);
    assert_eq!(r.quest().get("progress").and_then(|p| p.get("banish")).and_then(Json::as_u64), Some(2), "{}", r.quest());
    r.go(2.2, 2.0);
    r.press("colloquium", "talk");
    // Quest active: "About those bones..." and "Farewell.".
    r.press("colloquium", "choice_1");
    assert_eq!(r.node().as_deref(), Some("progress"));
    r.press("colloquium", "choice_1");
    assert_eq!(r.node().as_deref(), Some("thanks"));
    r.press("colloquium", "talk");
    r.ticks(4);
    let q = r.quest();
    assert_eq!(q.get("state").and_then(Json::as_str), Some("done"), "{q}");
    assert_eq!(r.items("bone"), 0, "handed over");
    assert_eq!(r.items("silver_key"), 1, "the reward");
    let known = r.player().props.get("abilities").and_then(|a| a.get("known")).cloned().unwrap();
    assert_eq!(known, Json::parse(r#"["flare", "ward"]"#).unwrap(), "the ward was learned");
    assert_eq!(r.player().props.get("colloquium").and_then(|c| c.get("flags")).and_then(|f| f.get("helped_sexton")), Some(&Json::Bool(true)));
    // Done: the Sexton says so.
    r.press("colloquium", "talk");
    let st = r.host.call_hook("colloquium/state", &Json::parse(r#"{"actor": "/ambulo/player"}"#).unwrap(), "test", "tester", &mut r.world, r.tick, HZ, &mut no_journal()).unwrap();
    assert_eq!(st.get("choices").and_then(Json::as_array).unwrap()[0].get("key").and_then(Json::as_str), Some("dialogue.sexton.all_quiet"));
    r.running();
}

#[test]
fn the_demo_plays_through_with_every_module_talking_by_events() {
    let mut r = Run::new();
    first_half(&mut r);
    second_half(&mut r);
    // Every module made sounds the engine would play: presentation only.
    let names: std::collections::BTreeSet<String> = r.host.take_sounds().into_iter().map(|s| format!("{}/{}", s.module, s.name)).collect();
    for s in ["inventarium/pickup", "facultas/flare", "colloquium/talk", "quaestio/quest_start", "quaestio/quest_done"] {
        assert!(names.contains(s), "{s} in {names:?}");
    }
}

#[test]
fn twin_runs_hash_the_same_at_every_step() {
    let (mut a, mut b) = (Run::new(), Run::new());
    assert_eq!(a.hashes(), b.hashes());
    first_half(&mut a);
    first_half(&mut b);
    assert_eq!(a.hashes(), b.hashes());
    second_half(&mut a);
    second_half(&mut b);
    assert_eq!(a.hashes(), b.hashes());
    assert_eq!(a.tick, b.tick);
}

#[test]
fn a_save_mid_quest_loads_and_finishes_exactly_like_the_unbroken_run() {
    let mut a = Run::new();
    first_half(&mut a);
    let bytes = a.host.save_state().expect("modules ran");
    let files = specula::persist::render(&a.world);
    let world = specula::persist::load_with(|f| files.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| specula::persist::PersistError::new("not_found", f.to_owned()))).unwrap().0;
    let mut b = Run { host: LogicHost::new(), world, tick: a.tick };
    for code in [
        ModuleCode::of::<ambulo::Ambulo>(),
        ModuleCode::of::<inventarium::Inventarium>(),
        ModuleCode::of::<facultas::Facultas>(),
        ModuleCode::of::<colloquium::Colloquium>(),
        ModuleCode::of::<quaestio::Quaestio>(),
        ModuleCode::of::<Exemplum>(),
    ] {
        b.host.load(code, Default::default(), Some(11), &mut b.world, b.tick, HZ, &mut no_journal()).unwrap();
    }
    let notes = b.host.restore_state(&bytes).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(b.hashes(), a.hashes(), "the loaded run starts where the saved one was");
    second_half(&mut a);
    second_half(&mut b);
    assert_eq!(b.hashes(), a.hashes());
}

#[test]
fn the_scene_data_and_every_module_sound_check_out() {
    let s = scene().unwrap();
    assert_eq!(s.iter().filter(|e| e.get("tags").and_then(Value::as_list).is_some_and(|t| t.iter().any(|t| t.as_str() == Some("wisp")))).count(), 3);
    // Every sound recipe the modules ship renders, within the engine's limit for module sounds.
    let games = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut n = 0;
    for m in ["inventarium", "facultas", "colloquium", "quaestio"] {
        for f in std::fs::read_dir(games.join(m).join("sounds")).unwrap().flatten() {
            let text = std::fs::read_to_string(f.path()).unwrap();
            assert!(text.len() as u64 <= specula::module_sounds::MAX_RECIPE_BYTES);
            let sound = specula_audio::load_recipe(&text, &specula_audio::RenderOptions::default()).unwrap_or_else(|e| panic!("{}: {e}", f.path().display()));
            assert!(sound.duration_s() <= specula::module_sounds::MAX_SOUND_S, "{}", f.path().display());
            n += 1;
        }
    }
    assert_eq!(n, 6);
}

#[test]
fn a_reset_builds_the_scene_again_and_status_reads_it() {
    let mut r = Run::new();
    r.go(-1.8, -3.0);
    r.press("inventarium", "interact");
    let st = r.host.call_hook("exemplum/status", &Json::Null, "test", "tester", &mut r.world, r.tick, HZ, &mut no_journal()).unwrap();
    assert_eq!(st.get("pickups").and_then(Json::as_object).map(|p| p.len()), Some(4), "a bone was picked up: 3 bones and the oil are left");
    let st = r.host.call_hook("exemplum/reset", &Json::Null, "test", "tester", &mut r.world, r.tick, HZ, &mut no_journal()).unwrap();
    assert_eq!(st.get("pickups").and_then(Json::as_object).map(|p| p.len()), Some(5));
    assert_eq!(r.items("bone"), 1, "the player's own state stays");
    // Wisps drift as a pure function of the tick.
    let w = r.world.find_path("/exemplum/wisp_1").unwrap();
    let p0 = r.world.get(w).unwrap().position;
    r.ticks(10);
    assert_ne!(r.world.get(w).unwrap().position, p0);
}

//! The inventory rules as plain functions, then the module in the engine's real module host
//! (linked in: the same ABI, messages and rollback as a loaded library).

use super::*;
use specula::logic::tools::no_journal;
use specula::logic::LogicHost;
use specula::world::{Shape, SpawnSpec, World, WorldError};
use specula_core::Json;
use specula_logic::ModuleCode;

fn data() -> Data {
    Data::parse(DATA).expect("the shipped items.json parses")
}

#[test]
fn stacks_fill_existing_slots_first_then_empty_ones() {
    let d = data();
    let mut inv = Inventory::new(4, 100_000);
    assert_eq!(inv.add(&d, "bone", 7, false).unwrap(), 7);
    assert_eq!(inv.add(&d, "grave_dust", 3, false).unwrap(), 3);
    assert_eq!(inv.add(&d, "bone", 5, false).unwrap(), 5, "tops the first stack to 10, then a new one");
    assert_eq!(inv.slots[0], Some(Stack { item: "bone".into(), count: 10 }));
    assert_eq!(inv.slots[1], Some(Stack { item: "grave_dust".into(), count: 3 }));
    assert_eq!(inv.slots[2], Some(Stack { item: "bone".into(), count: 2 }));
    assert_eq!(inv.count("bone"), 12);
    assert_eq!(inv.weight_g(&d), 12 * 300 + 3 * 20);
}

#[test]
fn slots_and_weight_both_limit_and_all_or_nothing_is_the_default() {
    let d = data();
    let mut inv = Inventory::new(2, 1000);
    // 1000 g holds 3 bones (300 g each), though two slots would hold 20.
    assert_eq!(inv.room_for(&d, "bone").unwrap(), 3);
    let e = inv.add(&d, "bone", 4, false).unwrap_err();
    assert_eq!(e.code, "no_room");
    assert_eq!(e.details.get("room").and_then(Value::as_u64), Some(3));
    assert_eq!(inv, Inventory::new(2, 1000), "a refused add changes nothing");
    assert_eq!(inv.add(&d, "bone", 4, true).unwrap(), 3, "partial takes what fits");
    // Slots, not weight: lanterns stack to 1 and two slots are full.
    let mut inv = Inventory::new(2, 100_000);
    assert_eq!(inv.add(&d, "lantern", 2, false).unwrap(), 2);
    assert_eq!(inv.add(&d, "lantern", 1, false).unwrap_err().code, "no_room");
    assert_eq!(inv.add(&d, "headstone", 1, false).unwrap_err().code, "no_room");
    assert_eq!(inv.add(&d, "nope", 1, false).unwrap_err().code, "unknown_item");
    assert_eq!(inv.add(&d, "bone", 0, false).unwrap_err().code, "invalid_field");
    assert_eq!(inv.add(&d, "bone", u64::MAX, true).unwrap_err().code, "invalid_field", "huge counts are refused before any work");
}

#[test]
fn take_is_all_or_nothing_from_the_last_slots() {
    let d = data();
    let mut inv = Inventory::new(3, 100_000);
    inv.add(&d, "bone", 15, false).unwrap();
    assert_eq!(inv.remove("bone", 16).unwrap_err().code, "not_enough");
    assert_eq!(inv.count("bone"), 15);
    inv.remove("bone", 7).unwrap();
    assert_eq!(inv.slots[0], Some(Stack { item: "bone".into(), count: 8 }));
    assert_eq!(inv.slots[1], None, "the last stack went first");
    assert_eq!(inv.remove("bone", 0).unwrap_err().code, "not_enough");
}

#[test]
fn moves_merge_swap_and_resizes_keep_items() {
    let d = data();
    let mut inv = Inventory::new(4, 100_000);
    inv.add(&d, "bone", 10, false).unwrap();
    inv.add(&d, "lantern", 1, false).unwrap();
    inv.slots[2] = Some(Stack { item: "bone".into(), count: 4 });
    inv.slots[0] = Some(Stack { item: "bone".into(), count: 8 });
    inv.move_slot(&d, 2, 0).unwrap();
    assert_eq!(inv.slots[0], Some(Stack { item: "bone".into(), count: 10 }), "merged up to the stack");
    assert_eq!(inv.slots[2], Some(Stack { item: "bone".into(), count: 2 }), "the rest stays");
    inv.move_slot(&d, 1, 2).unwrap();
    assert_eq!(inv.slots[2].as_ref().map(|s| s.item.as_str()), Some("lantern"), "swapped");
    assert_eq!(inv.move_slot(&d, 0, 9).unwrap_err().code, "invalid_field");
    assert_eq!(inv.resize(2, 100).unwrap_err().code, "not_empty");
    inv.move_slot(&d, 2, 3).unwrap();
    inv.move_slot(&d, 1, 3).unwrap();
    assert!(inv.slots[2].is_none());
}

#[test]
fn props_round_trip_and_hostile_props_are_refused() {
    let d = data();
    let mut inv = Inventory::new(5, 9000);
    inv.add(&d, "grave_dust", 60, false).unwrap();
    let v = inv.to_value(&d);
    assert_eq!(v.get("weight_g").and_then(Value::as_u64), Some(1200));
    assert_eq!(Inventory::from_value(&v).unwrap(), inv);
    for bad in [
        r#"{"format": 2, "slots": [null], "max_weight_g": 1}"#,
        r#"{"format": 1, "slots": [], "max_weight_g": 1}"#,
        r#"{"format": 1, "slots": [{"item": "bone", "count": 0}], "max_weight_g": 1}"#,
        r#"{"format": 1, "slots": [{"item": "Bone!", "count": 1}], "max_weight_g": 1}"#,
        r#"{"format": 1, "slots": [null], "max_weight_g": 0}"#,
        r#"{"format": 1, "slots": "x", "max_weight_g": 5}"#,
    ] {
        let v = Value::parse_json(bad).unwrap();
        assert_eq!(Inventory::from_value(&v).unwrap_err().code, "bad_inventory", "{bad}");
    }
    let many = format!(r#"{{"format": 1, "slots": [{}null], "max_weight_g": 5}}"#, "null, ".repeat(MAX_SLOTS as usize));
    assert_eq!(Inventory::from_value(&Value::parse_json(&many).unwrap()).unwrap_err().code, "bad_inventory");
}

#[test]
fn bad_data_names_its_place() {
    let cases = [
        (r#"{"format": "x"}"#, "format"),
        (r#"{"format": "specula.inventarium.v1", "defaults": {"slots": 99, "max_weight_g": 1}, "items": {}}"#, "defaults.slots"),
        (r#"{"format": "specula.inventarium.v1", "defaults": {"slots": 2, "max_weight_g": 1}, "items": {}}"#, "items"),
        (r#"{"format": "specula.inventarium.v1", "defaults": {"slots": 2, "max_weight_g": 1}, "items": {"a": {"stack": 0, "weight_g": 1, "name": {"key": "a", "text": "A"}}}}"#, "items.a.stack"),
        (r#"{"format": "specula.inventarium.v1", "defaults": {"slots": 2, "max_weight_g": 1}, "items": {"a": {"stack": 1, "weight_g": 1, "name": {"key": "A B", "text": "A"}}}}"#, "items.a.name.key"),
        (r#"{"format": "specula.inventarium.v1", "defaults": {"slots": 2, "max_weight_g": 1}, "items": {"B": {}}}"#, "items.B"),
    ];
    for (text, place) in cases {
        let e = Data::parse(text).unwrap_err();
        assert_eq!(e.code, "bad_data", "{text}");
        assert!(e.message.contains(place), "{} should name {place}", e.message);
    }
    // The largest data the caps allow parses quickly (cost before work: items are capped).
    let items: Vec<String> = (0..MAX_ITEMS).map(|i| format!(r#""i{i}": {{"stack": 9999, "weight_g": 1000000, "name": {{"key": "k{i}", "text": "{}"}}, "tags": [{}]}}"#, "x".repeat(400), vec!["\"t\""; 16].join(","))).collect();
    let text = format!(r#"{{"format": "specula.inventarium.v1", "defaults": {{"slots": 64, "max_weight_g": 100000000}}, "items": {{{}}}}}"#, items.join(","));
    assert!(text.len() < specula_sdk::value::MAX_JSON_BYTES);
    let d = Data::parse(&text).unwrap();
    assert_eq!(d.items.len(), MAX_ITEMS);
    let over: Vec<String> = (0..=MAX_ITEMS).map(|i| format!(r#""i{i}": {{"stack": 1, "weight_g": 1, "name": {{"key": "k", "text": "x"}}}}"#)).collect();
    let text = format!(r#"{{"format": "specula.inventarium.v1", "defaults": {{"slots": 1, "max_weight_g": 1}}, "items": {{{}}}}}"#, over.join(","));
    assert_eq!(Data::parse(&text).unwrap_err().code, "bad_data");
}

// --- The module in the engine's module host ---

pub(crate) struct Run {
    pub host: LogicHost,
    pub world: World,
    pub tick: u64,
}

impl Run {
    pub fn new() -> Run {
        let mut r = Run { host: LogicHost::new(), world: World::default(), tick: 0 };
        r.host.load(ModuleCode::of::<Inventarium>(), Default::default(), Some(7), &mut r.world, 0, 30, &mut no_journal()).expect("loads");
        r
    }

    pub fn ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.host.tick(&mut self.world, self.tick, 30, &mut no_journal());
            self.tick += 1;
        }
    }

    pub fn hook(&mut self, hook: &str, args: &str) -> Result<Json, WorldError> {
        self.host.call_hook(hook, &Json::parse(args).unwrap(), "test", "tester", &mut self.world, self.tick, 30, &mut no_journal())
    }

    pub fn spawn(&mut self, name: &str, pos: [f32; 3], tags: &[&str], props: &str) -> u64 {
        let mut s = SpawnSpec::new(Shape::Sphere);
        s.name = Some(name.into());
        s.position = specula_core::math::Vec3::new(pos[0], pos[1], pos[2]);
        s.tags = tags.iter().map(|t| (*t).to_owned()).collect();
        s.props = Json::parse(props).unwrap();
        self.world.spawn(s, "tester").unwrap().id
    }

    pub fn prop(&self, path: &str, key: &str) -> Json {
        let id = self.world.find_path(path).expect("exists");
        self.world.get(id).unwrap().props.get(key).cloned().unwrap_or(Json::Null)
    }

    pub fn events(&self, name: &str) -> Vec<Json> {
        let s = self.host.status(None, true).unwrap();
        s.get("events").and_then(Json::as_array).unwrap_or_default().iter().filter(|e| e.get("name").and_then(Json::as_str) == Some(name)).cloned().collect()
    }
}

#[test]
fn hooks_give_take_move_and_inspect_through_the_host() {
    let mut r = Run::new();
    r.spawn("hero", [0.0; 3], &[], "{}");
    let out = r.hook("inventarium/give", r#"{"owner": "/hero", "item": "bone", "count": 12}"#).unwrap();
    assert_eq!(out.get("added").and_then(Json::as_u64), Some(12));
    let inv = r.prop("/hero", "inventory");
    assert_eq!(inv.get("weight_g").and_then(Json::as_u64), Some(3600));
    assert_eq!(inv.get("slots").and_then(Json::as_array).map(|s| s.len()), Some(12), "opened with the data's defaults");
    let e = r.hook("inventarium/take", r#"{"owner": "/hero", "item": "bone", "count": 99}"#).unwrap_err();
    assert_eq!(e.code, "hook_failed");
    assert!(format!("{e:?}").contains("not_enough"), "{e:?}");
    assert_eq!(r.prop("/hero", "inventory"), inv, "a failed hook changes nothing");
    r.hook("inventarium/move", r#"{"owner": "/hero", "from": 1, "to": 5}"#).unwrap();
    let look = r.hook("inventarium/inspect", r#"{"owner": "/hero"}"#).unwrap();
    assert_eq!(look.get("names").and_then(Json::as_array).unwrap()[5].get("key").and_then(Json::as_str), Some("item.bone.name"));
    assert_eq!(look.get("free_slots").and_then(Json::as_u64), Some(10));
    let e = r.hook("inventarium/give", r#"{"owner": "/nobody", "item": "bone", "count": 1}"#).unwrap_err();
    assert!(format!("{e:?}").contains("not_found"), "{e:?}");
    // The changes went out as events, seen by every module next tick.
    r.ticks(1);
    let changed = r.events("inventarium_changed");
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert_eq!(changed[0].get("data").and_then(|d| d.get("count")).and_then(Json::as_u64), Some(12));
}

/// Another module asks for items through events; the inventory module applies them next tick.
#[derive(Default)]
struct Asker;
impl Module for Asker {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "asker", ..ModuleInfo::default() }
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        if cx.tick() == 1 {
            cx.emit("inventarium_give", Value::map().with("owner", "/hero").with("item", "lantern_oil").with("count", 3))?;
            cx.emit("inventarium_give", Value::map().with("owner", "/hero").with("item", "headstone").with("count", 1))?;
        }
        if cx.tick() == 3 {
            cx.emit("inventarium_take", Value::map().with("owner", "/hero").with("item", "lantern_oil").with("count", 2))?;
        }
        Ok(())
    }
}

#[test]
fn other_modules_ask_through_events_and_refusals_come_back_as_events() {
    let mut r = Run::new();
    r.spawn("hero", [0.0; 3], &[], "{}");
    r.host.load(ModuleCode::of::<Asker>(), Default::default(), None, &mut r.world, 0, 30, &mut no_journal()).unwrap();
    r.ticks(5);
    assert_eq!(r.prop("/hero", "inventory").get("weight_g").and_then(Json::as_u64), Some(500), "3 oil given at tick 2, 2 taken at tick 4 (asked at 3)");
    let refused = r.events("inventarium_refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].get("data").and_then(|d| d.get("code")).and_then(Json::as_str), Some("no_room"), "a 45 kg headstone");
}

#[test]
fn interact_picks_up_the_nearest_pickup_in_reach_and_leaves_what_does_not_fit() {
    let mut r = Run::new();
    let mut g = SpawnSpec::new(Shape::Group);
    g.name = Some("ambulo".into());
    r.world.spawn(g, "tester").unwrap();
    let mut p = SpawnSpec::new(Shape::Sphere);
    p.name = Some("player".into());
    p.parent = Some(specula::world::EntityKey::Path("/ambulo".into()));
    r.world.spawn(p, "tester").unwrap();
    r.hook("inventarium/open", r#"{"owner": "/ambulo/player", "slots": 1, "max_weight_g": 100000}"#).unwrap();
    r.spawn("far_bones", [3.0, 0.0, 0.0], &["pickup"], r#"{"pickup": {"item": "bone", "count": 2}}"#);
    r.spawn("near_dust", [1.0, 0.0, 0.0], &["pickup"], r#"{"pickup": {"item": "grave_dust", "count": 70}}"#);
    r.host.hold("inventarium", &Json::parse(r#"{"interact": true}"#).unwrap(), 1).unwrap();
    r.ticks(1);
    // One slot of 50 dust: 20 stay on the ground.
    let inv = r.prop("/ambulo/player", "inventory");
    assert_eq!(inv.get("slots").and_then(Json::as_array).unwrap()[0].get("count").and_then(Json::as_u64), Some(50));
    assert_eq!(r.prop("/near_dust", "pickup").get("count").and_then(Json::as_u64), Some(20));
    assert!(r.world.find_path("/far_bones").is_ok(), "out of reach");
    // Drop the dust, pick up the rest: the pickup is gone.
    r.hook("inventarium/open", r#"{"owner": "/ambulo/player", "slots": 2}"#).unwrap();
    r.host.hold("inventarium", &Json::parse(r#"{"interact": false}"#).unwrap(), 1).unwrap();
    r.ticks(1);
    r.host.hold("inventarium", &Json::parse(r#"{"interact": true}"#).unwrap(), 1).unwrap();
    r.ticks(1);
    assert!(r.world.find_path("/near_dust").is_err(), "picked up whole");
    assert_eq!(r.prop("/ambulo/player", "inventory").get("weight_g").and_then(Json::as_u64), Some(70 * 20));
    // The pickup asked for its sound; the host queued it for the engine to play.
    let sounds = r.host.take_sounds();
    assert_eq!(sounds.len(), 2);
    assert_eq!((sounds[0].module.as_str(), sounds[0].name.as_str()), ("inventarium", "pickup"));
}

#[test]
fn twin_runs_match_and_a_saved_run_continues_exactly() {
    let script = |r: &mut Run| {
        r.spawn("hero", [0.0; 3], &[], "{}");
        r.hook("inventarium/give", r#"{"owner": "/hero", "item": "bone", "count": 9}"#).unwrap();
        r.ticks(5);
        r.hook("inventarium/take", r#"{"owner": "/hero", "item": "bone", "count": 4}"#).unwrap();
        r.ticks(5);
    };
    let (mut a, mut b) = (Run::new(), Run::new());
    script(&mut a);
    script(&mut b);
    assert_eq!(a.world.state_hash(), b.world.state_hash());
    assert_eq!(a.host.state_hash(), b.host.state_hash());
    // Save a, load it into a fresh host and world, and carry on: identical.
    let bytes = a.host.save_state().unwrap();
    let files = specula::persist::render(&a.world);
    let world = specula::persist::load_with(|f| files.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| specula::persist::PersistError::new("not_found", f.to_owned()))).unwrap().0;
    let mut c = Run { host: LogicHost::new(), world, tick: a.tick };
    c.host.load(ModuleCode::of::<Inventarium>(), Default::default(), Some(7), &mut c.world, c.tick, 30, &mut no_journal()).unwrap();
    assert!(c.host.restore_state(&bytes).unwrap().is_empty());
    for _ in 0..3 {
        a.hook("inventarium/give", r#"{"owner": "/hero", "item": "grave_dust", "count": 5}"#).unwrap();
        c.hook("inventarium/give", r#"{"owner": "/hero", "item": "grave_dust", "count": 5}"#).unwrap();
        a.ticks(2);
        c.ticks(2);
        assert_eq!(a.world.state_hash(), c.world.state_hash());
        assert_eq!(a.host.state_hash(), c.host.state_hash());
    }
}

/// The player at `x` (and the inventory it opens).
fn player_at(r: &mut Run, x: f32) {
    let mut g = SpawnSpec::new(Shape::Group);
    g.name = Some("ambulo".into());
    r.world.spawn(g, "tester").unwrap();
    let mut p = SpawnSpec::new(Shape::Sphere);
    p.name = Some("player".into());
    p.parent = Some(specula::world::EntityKey::Path("/ambulo".into()));
    p.position = specula_core::math::Vec3::new(x, 0.0, 0.0);
    r.world.spawn(p, "tester").unwrap();
    r.hook("inventarium/open", r#"{"owner": "/ambulo/player", "slots": 4, "max_weight_g": 100000}"#).unwrap();
}

/// Review of m5-gmods (major 3): with 300 pickups, the one the player stands on (the 300th by
/// id) was never looked at (the first 256 by id filled the cap before the reach filter).
#[test]
fn a_pickup_past_256_others_is_picked_up() {
    let mut r = Run::new();
    for i in 0..300 {
        r.spawn(&format!("coin{i}"), [100.0 + i as f32, 0.0, 0.0], &["pickup"], r#"{"pickup": {"item": "bone", "count": 1}}"#);
    }
    player_at(&mut r, 399.0);
    r.host.hold("inventarium", &Json::parse(r#"{"interact": true}"#).unwrap(), 1).unwrap();
    r.ticks(1);
    assert!(r.world.find_path("/coin299").is_err(), "picked up: {:?}", r.prop("/ambulo/player", "inventory"));
}

/// Review of m5-gmods (major 3): a press reads the tag index, never the world: with 100,000
/// untagged entities it costs what it costs in an empty world (a walk of them took about 3 ms a
/// press in a debug build).
#[test]
#[allow(clippy::disallowed_methods, reason = "the test times a press; nothing simulated reads the clock")]
fn a_press_never_walks_the_world() {
    fn fastest_press(extra: u32) -> std::time::Duration {
        let mut r = Run::new();
        player_at(&mut r, 0.0);
        for i in 0..extra {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some(format!("e{i}"));
            r.world.spawn(s, "tester").unwrap();
        }
        r.ticks(2);
        let mut best = std::time::Duration::MAX;
        for _ in 0..7 {
            r.host.hold("inventarium", &Json::parse(r#"{"interact": true}"#).unwrap(), 1).unwrap();
            let t = std::time::Instant::now(); // determinism: allowed: the test times a press, nothing simulated reads it
            r.ticks(1);
            best = best.min(t.elapsed());
            r.ticks(1);
        }
        best
    }
    let (empty, full) = (fastest_press(0), fastest_press(100_000));
    assert!(full < empty * 2 + std::time::Duration::from_micros(1500), "a press took {full:?} with 100k entities, {empty:?} without");
}

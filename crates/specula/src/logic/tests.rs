//! The module host with modules linked into the test (same ABI and messages as a library).

use super::tools::no_journal;
use super::*;
use specula_sdk::{Cx, HookDef, Id, Module, ModuleInfo, Patch, SdkError, Spawn, Vec3};

const ACTIONS: &str = r#"{"format": "specula.input.v1", "actions": [
    {"name": "move", "kind": "axis2", "output": "value", "bindings": ["Pad.LeftStick"]},
    {"name": "jump", "kind": "button", "bindings": ["Key.Space"]}
]}"#;

/// Moves `/walker` along +X at `SPEED` × the move action; counts ticks in its state store and
/// draws from an RNG stream every tick.
fn walk<const SPEED: u32>(cx: &mut Cx) -> Result<Id, SdkError> {
    let id = cx.find("/walker")?.ok_or_else(|| SdkError::new("not_found", "no walker"))?;
    let e = cx.entity(id)?.expect("exists");
    let step = cx.action("move").value[0] * SPEED as f32 * cx.dt();
    let roll = cx.random_below("dice", 6)?;
    let ticks = cx.state("ticks")?.as_u64().unwrap_or(0) + 1;
    cx.set_state("ticks", ticks)?;
    cx.set(id, Patch::new().position(e.position() + Vec3::new(step, 0.0, 0.0)).props(Value::map().with("roll", roll)))?;
    if cx.action("jump").pressed {
        cx.emit("jumped", Value::map().with("at", cx.tick()))?;
    }
    Ok(id)
}

fn start_walker(cx: &mut Cx) -> Result<(), SdkError> {
    if cx.find("/walker")?.is_none() {
        cx.spawn(Spawn::new("sphere").name("walker").tags(&["walker"]))?;
    }
    cx.set_state("reloaded", cx.reloaded())?;
    cx.set_state("generation", cx.generation())
}

fn info(version: &'static str) -> ModuleInfo {
    ModuleInfo {
        name: "walker",
        version,
        actions: ACTIONS,
        hooks: vec![HookDef { name: "nudge", doc: "moves the walker by dx", args: r#"{"type": "object"}"# }],
        ..ModuleInfo::default()
    }
}

#[derive(Default)]
struct WalkerV1;
impl Module for WalkerV1 {
    fn describe(&self) -> ModuleInfo {
        info("1")
    }
    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        start_walker(cx)
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        walk::<1>(cx).map(drop)
    }
    fn hook(&self, cx: &mut Cx, name: &str, args: &Value) -> Result<Value, SdkError> {
        assert_eq!(name, "nudge");
        let id = cx.find("/walker")?.expect("walker");
        let e = cx.entity(id)?.expect("exists");
        let dx = args.get("dx").and_then(Value::as_f32).ok_or_else(|| SdkError::new("invalid_field", "dx"))?;
        cx.set(id, Patch::new().position(e.position() + Vec3::new(dx, 0.0, 0.0)))?;
        if dx < 0.0 {
            return Err(SdkError::new("invalid_field", "backwards"));
        }
        Ok(Value::from(e.position().x + dx))
    }
}

/// The "edited and rebuilt" walker: twice as fast.
#[derive(Default)]
struct WalkerV2;
impl Module for WalkerV2 {
    fn describe(&self) -> ModuleInfo {
        info("2")
    }
    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        start_walker(cx)
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        walk::<2>(cx).map(drop)
    }
}

/// A broken build: changes things, then panics on its third tick.
#[derive(Default)]
struct WalkerBomb;
impl Module for WalkerBomb {
    fn describe(&self) -> ModuleInfo {
        info("bomb")
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        walk::<5>(cx)?;
        let mine = cx.state("bomb_ticks")?.as_u64().unwrap_or(0) + 1;
        cx.set_state("bomb_ticks", mine)?;
        cx.spawn(Spawn::new("cube").name(&format!("debris_{mine}")))?;
        if mine == 3 {
            panic!("bomb went off at tick {}", cx.tick());
        }
        Ok(())
    }
}

/// Fails (an error, not a panic) every other tick.
#[derive(Default)]
struct Flaky;
impl Module for Flaky {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "flaky", order: -1, ..ModuleInfo::default() }
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        cx.set_state("t", cx.tick())?;
        cx.emit("flaky_tick", Value::Null)?;
        if cx.tick() % 2 == 1 {
            return Err(SdkError::new("odd", "odd ticks fail"));
        }
        Ok(())
    }
}

#[derive(Default)]
struct BadName;
impl Module for BadName {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "Bad Name", ..ModuleInfo::default() }
    }
}

struct Run {
    host: LogicHost,
    world: World,
    tick: u64,
}

impl Run {
    fn new() -> Run {
        Run { host: LogicHost::new(), world: World::default(), tick: 0 }
    }

    fn load(&mut self, code: ModuleCode) -> Result<Json, WorldError> {
        self.host.load(code, Origin::default(), None, &mut self.world, self.tick, 30, &mut no_journal())
    }

    fn ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.host.tick(&mut self.world, self.tick, 30, &mut no_journal());
            self.tick += 1;
        }
    }

    fn walker(&self) -> &crate::world::Entity {
        let id = self.world.find_path("/walker").expect("walker");
        self.world.get(id).expect("walker")
    }

    fn state(&self, module: &str, key: &str) -> Value {
        self.host.shared.get(module).and_then(|s| s.store.get(key)).cloned().unwrap_or_default()
    }
}

#[test]
fn reload_keeps_the_world_and_the_state_and_takes_the_new_code() {
    let mut r = Run::new();
    let report = r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    assert_eq!(report.get("event").and_then(Json::as_str), Some("load"));
    assert_eq!(r.state("walker", "reloaded"), Value::Bool(false));
    r.host.hold("walker", &Json::parse(r#"{"move": [1, 0]}"#).unwrap(), 1000).unwrap();
    r.ticks(30);
    let id = r.walker().id;
    let x1 = r.walker().position.x;
    assert!((x1 - 1.0).abs() < 1e-4, "1 m/s for 1 s: {x1}");

    // "Edit and rebuild": the same module name, new code.
    let report = r.load(ModuleCode::of::<WalkerV2>()).unwrap();
    assert_eq!(report.get("event").and_then(Json::as_str), Some("reload"));
    assert_eq!(report.get("generation").and_then(Json::as_u64), Some(2));
    assert_eq!(r.walker().id, id, "same entity");
    assert_eq!(r.walker().position.x, x1, "nothing moved at the reload");
    assert_eq!(r.state("walker", "ticks"), Value::Int(30), "state store survived");
    assert_eq!(r.state("walker", "reloaded"), Value::Bool(true));
    assert_eq!(r.state("walker", "generation"), Value::Int(2));
    r.ticks(30);
    let x2 = r.walker().position.x;
    assert!((x2 - x1 - 2.0).abs() < 1e-4, "the agent's held input carried over, at the new speed: {x2}");
    assert_eq!(r.state("walker", "ticks"), Value::Int(60));
}

#[test]
fn a_panic_disables_the_module_and_changes_nothing() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    r.host.hold("walker", &Json::parse(r#"{"move": [1, 0]}"#).unwrap(), 1000).unwrap();
    r.ticks(3);
    r.load(ModuleCode::of::<WalkerBomb>()).unwrap();
    r.ticks(2);
    let contents = |w: &World| w.entities().map(|e| e.content_hash()).collect::<Vec<_>>();
    let (x, len, ticks, before) = (r.walker().position.x, r.world.len(), r.state("walker", "ticks"), contents(&r.world));
    let rng_before = r.host.shared["walker"].rngs.get("dice").copied();
    r.ticks(1); // the third bomb tick panics after moving, spawning and writing state
    assert!(!r.host.running("walker"));
    assert_eq!(r.walker().position.x, x, "move rolled back");
    assert_eq!(r.world.len(), len, "spawn rolled back");
    // Every entity exactly as before (only the id counter moved on: ids are never reused).
    assert_eq!(contents(&r.world), before, "the world is exactly as before the tick");
    assert_eq!(r.state("walker", "ticks"), ticks, "state writes rolled back");
    assert_eq!(r.host.shared["walker"].rngs.get("dice").copied(), rng_before, "RNG draws rolled back");
    let status = r.host.status(Some("walker"), false).unwrap();
    let m = &status.get("modules").and_then(Json::as_array).unwrap()[0];
    assert_eq!(m.get("status").and_then(Json::as_str), Some("disabled"));
    let why = m.get("disabled_by").unwrap();
    assert_eq!(why.get("failure").and_then(Json::as_str), Some("panic"));
    assert_eq!(why.get("message").and_then(Json::as_str), Some("bomb went off at tick 5"));
    assert!(why.get("location").and_then(Json::as_str).is_some_and(|l| l.contains("tests.rs")), "{why}");
    // Disabled: later ticks don't run it; a fixed build brings it back.
    r.ticks(5);
    assert_eq!(r.walker().position.x, x);
    r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    assert!(r.host.running("walker"));
    r.ticks(1);
    assert!(r.walker().position.x > x);
}

#[test]
fn errors_roll_back_but_keep_running_and_events_arrive_next_tick() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<Flaky>()).unwrap();
    r.ticks(4);
    let status = r.host.status(None, true).unwrap();
    let m = &status.get("modules").and_then(Json::as_array).unwrap()[0];
    assert_eq!(m.get("status").and_then(Json::as_str), Some("running"));
    assert_eq!(m.get("failures").and_then(Json::as_u64), Some(2));
    assert_eq!(m.get("last_error").and_then(|e| e.get("code")).and_then(Json::as_str), Some("odd"));
    assert_eq!(r.state("flaky", "t"), Value::Int(2), "tick 3's write was undone");
    // Events from the good ticks (0 and 2) only; tick 2's arrived at tick 3.
    let events = status.get("events").and_then(Json::as_array).unwrap();
    assert_eq!(events.len(), 2, "{status}");
}

#[test]
fn broken_builds_never_replace_working_ones() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    let e = r.load(ModuleCode::of::<BadName>()).unwrap_err();
    assert_eq!(e.code, "bad_module_name");
    assert!(r.host.running("walker"));
    assert_eq!(r.host.modules(), vec!["walker".to_owned()]);
}

#[test]
fn modules_tick_in_order_then_name() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    r.load(ModuleCode::of::<Flaky>()).unwrap();
    assert_eq!(r.host.modules(), vec!["flaky".to_owned(), "walker".to_owned()], "flaky has order -1");
}

#[test]
fn hooks_are_called_by_name_and_every_use_is_logged() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    let hooks = r.host.hooks();
    assert_eq!(hooks.iter().map(HookInfo::qualified).collect::<Vec<_>>(), vec!["walker/nudge"]);
    let args = Json::parse(r#"{"dx": 2.5}"#).unwrap();
    let call = |r: &mut Run, hook: &str, args: &Json, reason: &str| {
        r.host.call_hook(hook, args, reason, "lex", &mut r.world, r.tick, 30, &mut no_journal())
    };
    assert_eq!(call(&mut r, "walker/nudge", &args, "knockback has no Lex effect yet").unwrap(), Json::from(2.5));
    assert_eq!(r.walker().position.x, 2.5);
    assert_eq!(call(&mut r, "walker/nudge", &args, " ").unwrap_err().code, "invalid_field", "a reason is required");
    assert_eq!(call(&mut r, "walker/jump", &args, "x").unwrap_err().code, "no_hook");
    assert_eq!(call(&mut r, "nobody/x", &args, "x").unwrap_err().code, "no_module");
    let back = call(&mut r, "walker/nudge", &Json::parse(r#"{"dx": -1}"#).unwrap(), "test").unwrap_err();
    assert_eq!(back.code, "hook_failed");
    assert_eq!(r.walker().position.x, 2.5, "the failed hook's move was rolled back");
    let status = r.host.status(None, false).unwrap();
    assert_eq!(status.get("hook_counts").and_then(|c| c.get("walker/nudge")).and_then(Json::as_u64), Some(2));
    let uses = status.get("hook_uses").and_then(Json::as_array).unwrap();
    assert_eq!(uses[0].get("reason").and_then(Json::as_str), Some("knockback has no Lex effect yet"));
    assert_eq!(uses[0].get("caller").and_then(Json::as_str), Some("lex"));
}

#[test]
fn same_inputs_same_world() {
    let run = || {
        let mut r = Run::new();
        r.load(ModuleCode::of::<WalkerV1>()).unwrap();
        r.load(ModuleCode::of::<Flaky>()).unwrap();
        r.host.hold("walker", &Json::parse(r#"{"move": [0.7, 0], "jump": true}"#).unwrap(), 17).unwrap();
        r.ticks(40);
        (r.world.state_hash(), r.state("walker", "ticks"), r.walker().props.clone())
    };
    assert_eq!(run(), run());
}

#[test]
fn unload_keeps_state_unless_forgotten() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    r.ticks(3);
    r.host.unload("walker", false, &mut r.world, r.tick, 30, &mut no_journal()).unwrap();
    assert!(r.host.is_empty());
    assert_eq!(r.state("walker", "ticks"), Value::Int(3));
    let report = r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    assert_eq!(report.get("generation").and_then(Json::as_u64), Some(2));
    r.host.unload("walker", true, &mut r.world, r.tick, 30, &mut no_journal()).unwrap();
    assert!(r.host.shared.is_empty());
    assert_eq!(r.host.unload("walker", true, &mut r.world, r.tick, 30, &mut no_journal()).unwrap_err().code, "no_module");
}

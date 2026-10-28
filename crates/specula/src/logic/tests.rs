//! The module host with modules linked into the test (same ABI and messages as a library).

use super::tools::no_journal;
use super::*;
use crate::world::{Recording, World};
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

/// Spawns, then fails on even ticks (the reviewer's probe for ids used by failed calls).
#[derive(Default)]
struct SpawnThenFail;
impl Module for SpawnThenFail {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "spawnfail", ..ModuleInfo::default() }
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        cx.spawn(Spawn::new("cube").name(&format!("tmp_{}", cx.tick())))?;
        if cx.tick() % 2 == 0 {
            return Err(SdkError::new("nope", "fails after spawning"));
        }
        Ok(())
    }
}

/// A new walker build that can't start (after changing the world and its state).
#[derive(Default)]
struct WalkerBadStart;
impl Module for WalkerBadStart {
    fn describe(&self) -> ModuleInfo {
        info("bad start")
    }
    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        cx.spawn(Spawn::new("cube").name("junk"))?;
        cx.set_state("ticks", 999)?;
        Err(SdkError::new("broken", "this build can't start"))
    }
}

/// Requests forever.
#[derive(Default)]
struct Runaway;
impl Module for Runaway {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "runaway", ..ModuleInfo::default() }
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        loop {
            // A module that swallows the refusal doesn't escape it either.
            let _ = cx.log("spin");
        }
    }
}

/// Emits a lot, or big things, when asked.
#[derive(Default)]
struct Chatty;
impl Module for Chatty {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo {
            name: "chatty",
            hooks: vec![HookDef { name: "many", doc: "200 events", args: "" }, HookDef { name: "big", doc: "one 5 kB event", args: "" }],
            ..ModuleInfo::default()
        }
    }
    fn hook(&self, cx: &mut Cx, name: &str, _args: &Value) -> Result<Value, SdkError> {
        if name == "big" {
            cx.emit("big", "x".repeat(5000))?;
        } else {
            for i in 0..200 {
                cx.emit("e", i)?;
            }
        }
        Ok(Value::Null)
    }
}

#[test]
fn failed_calls_still_journal_the_ids_they_used() {
    use crate::swarm::Swarm;
    let mut swarm = Swarm::default();
    let mut world = World::default();
    world.set_recording(Recording::Full);
    let mut host = LogicHost::new();
    for tick in 0..5 {
        let mut commit = |w: &mut dyn crate::write_policy::WorldWrite, _: Mark, agent: &str, phase: Phase| {
            crate::write_policy::journal_exempt(w, &mut swarm, tick, crate::write_policy::Exempt::Module { agent, kind: tools::kind(phase) });
            Ok(())
        };
        if tick == 0 {
            host.load(ModuleCode::of::<SpawnThenFail>(), Origin::default(), None, &mut world, 0, 30, &mut commit).unwrap();
        }
        host.tick(&mut world, tick, 30, &mut commit);
    }
    assert_eq!(world.len(), 2, "ticks 1 and 3 kept their cubes");
    let replayed = swarm.journal().replay().expect("replayable");
    assert_eq!(replayed.state_hash(), world.state_hash(), "world_hash verify matches after failed spawns");
}

#[test]
fn a_build_that_fails_to_start_leaves_the_running_one() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    r.ticks(3);
    let (len, ticks) = (r.world.len(), r.state("walker", "ticks"));
    let e = r.load(ModuleCode::of::<WalkerBadStart>()).unwrap_err();
    assert_eq!(e.code, "start_failed");
    assert!(e.message.contains("running build stays"), "{}", e.message);
    assert!(r.host.running("walker"));
    assert_eq!(r.host.slots["walker"].info.version, "1", "the old build is still the one loaded");
    assert_eq!(r.host.shared["walker"].loads, 1, "no generation was used up");
    assert_eq!((r.world.len(), r.state("walker", "ticks")), (len, ticks), "its start's changes were rolled back");
    r.ticks(1);
    assert_eq!(r.state("walker", "ticks"), Value::Int(4), "and the old build keeps ticking");
}

#[test]
fn a_runaway_module_is_disabled_after_one_bounded_call() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<Runaway>()).unwrap();
    r.ticks(3);
    assert!(!r.host.running("runaway"));
    let status = r.host.status(Some("runaway"), false).unwrap();
    let m = &status.get("modules").and_then(Json::as_array).unwrap()[0];
    assert_eq!(m.get("disabled_by").and_then(|d| d.get("code")).and_then(Json::as_str), Some("too_many_requests"));
    assert_eq!(m.get("ticks").and_then(Json::as_u64), Some(1), "called once, then never again");
    let lines = status.get("log").and_then(Json::as_array).unwrap().len();
    assert!(lines <= 8, "the log stays bounded");
}

#[test]
fn events_are_bounded_in_size_and_number() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<Chatty>()).unwrap();
    let call = |r: &mut Run, hook: &str| r.host.call_hook(hook, &Json::Null, "test", "t", &mut r.world, r.tick, 30, &mut no_journal());
    let e = call(&mut r, "chatty/big").unwrap_err();
    assert_eq!(e.detail("failure").and_then(|f| f.get("code")).and_then(Json::as_str), Some("event_too_big"));
    // Paused (no ticks to deliver them): pending events fill up, then emits are refused.
    for _ in 0..(MAX_PENDING_EVENTS / 200) {
        call(&mut r, "chatty/many").unwrap();
    }
    let e = call(&mut r, "chatty/many").unwrap_err();
    assert_eq!(e.detail("failure").and_then(|f| f.get("code")).and_then(Json::as_str), Some("too_many_events"));
    assert!(r.host.pending_events.len() <= MAX_PENDING_EVENTS);
}

#[test]
fn claims_guard_module_loads_unloads_and_hooks() {
    use crate::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
    use crate::journal::Writer;
    use crate::swarm::Swarm;
    use specula_protocol::{Content, SessionInfo, ToolOutput};
    let session = |id: &str| SessionInfo { id: id.into(), client_name: "t".into(), client_version: "1".into(), protocol_version: "x".into() };
    let (so, si) = (session("owner0001"), session("intrud002"));
    let live = vec![so.clone(), si.clone()];
    let mut swarm = Swarm::default();
    swarm.hello(&so, "owner".into(), "builder".into(), None, &live).unwrap();
    swarm.hello(&si, "intruder".into(), "builder".into(), None, &live).unwrap();
    let e = swarm.hello(&session("sneaky03"), "module:walker2".into(), "builder".into(), None, &live).unwrap_err();
    assert_eq!(e.code, "name_reserved", "the whole module: prefix is reserved, loaded or not");
    let (ko, ki) = (swarm.key(&so), swarm.key(&si));
    let (owner, intruder) = (Writer::new("owner", Some(&ko)), Writer::new("intruder", Some(&ki)));
    let mut world = World::default();
    world.set_recording(Recording::Full);
    let clock = crate::game::Clock::new(30);
    let mut host = LogicHost::new();
    let load = |code| tools::LogicCall::Load { code, origin: Origin::default(), seed: None, open_ms: 0.0 };
    let hook = || tools::LogicCall::Hook { hook: "walker/nudge".into(), args: Json::parse(r#"{"dx": 1}"#).unwrap(), reason: "test".into() };
    let text = |o: &ToolOutput| match &o.content[0] {
        Content::Text(t) => t.clone(),
        _ => String::new(),
    };
    let out = tools::handle(&mut host, load(ModuleCode::of::<WalkerV1>()), &mut world, &clock, &mut swarm, owner);
    assert!(!out.is_error, "{}", text(&out));
    swarm.take_claim(&world, &so, ClaimSpec { scope: Scope::path("/walker").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    let x = |w: &World| w.get(w.find_path("/walker").unwrap()).unwrap().position.x;
    // The intruder can't replace or unload the owner's module, nor reach into the claim through a hook.
    for call in [load(ModuleCode::of::<WalkerV2>()), tools::LogicCall::Unload { module: "walker".into(), forget: false }, hook()] {
        let out = tools::handle(&mut host, call, &mut world, &clock, &mut swarm, intruder);
        assert!(out.is_error && text(&out).contains("claimed"), "{}", text(&out));
    }
    assert_eq!((x(&world), host.slots["walker"].info.version.as_str()), (0.0, "1"), "nothing changed");
    let out = tools::handle(&mut host, hook(), &mut world, &clock, &mut swarm, owner);
    assert!(!out.is_error, "{}", text(&out));
    assert_eq!(x(&world), 1.0);
    let out = tools::handle(&mut host, load(ModuleCode::of::<WalkerV2>()), &mut world, &clock, &mut swarm, owner);
    assert!(!out.is_error, "the owner may replace it: {}", text(&out));
    let replayed = swarm.journal().replay().expect("replayable");
    assert_eq!(replayed.state_hash(), world.state_hash());
}

/// Save games: a host restored from `save_state` (stores, RNG positions, pending events, held
/// input) continues exactly like the one that saved.
#[test]
fn a_saved_host_continues_exactly() {
    let mut a = Run::new();
    a.load(ModuleCode::of::<WalkerV1>()).unwrap();
    a.host.hold("walker", &Json::parse(r#"{"move": [1, 0], "jump": true}"#).unwrap(), 25).unwrap();
    a.ticks(20);
    let bytes = a.host.save_state().expect("a module ran");
    let files = crate::persist::render(&a.world);
    let world = crate::persist::load_with(|f| {
        files.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| crate::persist::PersistError::new("not_found", f.to_owned()))
    })
    .unwrap()
    .0;
    let mut b = Run { host: LogicHost::new(), world, tick: a.tick };
    b.load(ModuleCode::of::<WalkerV1>()).unwrap();
    assert_ne!(b.host.state_hash(), a.host.state_hash(), "a fresh load starts its own state");
    let notes = b.host.restore_state(&bytes).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(b.host.state_hash(), a.host.state_hash());
    assert_eq!(b.world.state_hash(), a.world.state_hash());
    for t in 0..40 {
        a.ticks(1);
        b.ticks(1);
        assert_eq!(b.host.state_hash(), a.host.state_hash(), "module state diverged {t} ticks after the load");
        assert_eq!(b.world.state_hash(), a.world.state_hash(), "the world diverged {t} ticks after the load");
    }
    assert_eq!(b.state("walker", "ticks"), Value::Int(60));
    // Damaged bytes are an error and change nothing; a host without the module says so.
    let before = b.host.state_hash();
    for cut in (0..bytes.len()).step_by(5) {
        assert!(b.host.restore_state(&bytes[..cut]).is_err());
    }
    assert_eq!(b.host.state_hash(), before);
    let mut c = LogicHost::new();
    let notes = c.restore_state(&bytes).unwrap();
    assert!(notes.iter().any(|n| n.get("module").and_then(Json::as_str) == Some("walker")), "{notes:?}");
}

/// Asks for 100 new RNG streams a tick (the anti-pattern: a stream per thing) and notes the first
/// refusal's code in its state store; fails every fifth tick after drawing (rolled back).
#[derive(Default)]
struct StreamHog;
impl Module for StreamHog {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "hog", ..ModuleInfo::default() }
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        let t = cx.tick();
        for i in 0..100 {
            if let Err(e) = cx.random_u32(&format!("thing_{t}_{i}")) {
                if cx.state("refused")?.is_null() {
                    cx.set_state("refused", e.code.as_str())?;
                }
                break;
            }
        }
        // Old streams still draw at the cap.
        cx.random_u32("thing_0_0")?;
        if t % 5 == 4 {
            return Err(SdkError::new("odd", "drew, then failed"));
        }
        Ok(())
    }
}

/// The streams' incrementally kept hash, recomputed from scratch.
fn rng_sum_from_scratch(s: &Shared) -> u64 {
    s.rngs().iter().fold(0u64, |sum, (k, r)| sum.wrapping_add(super::stream_hash(k, r)))
}

/// A module's RNG streams are capped (`rng_full`, like `state_full`), a save at the cap loads
/// (the writer's cap is the reader's), and the per-tick hash keeps up with draws and rollbacks.
#[test]
fn rng_streams_are_capped_and_a_save_at_the_cap_loads() {
    let mut a = Run::new();
    a.load(ModuleCode::of::<StreamHog>()).unwrap();
    a.ticks(20);
    let s = &a.host.shared["hog"];
    assert_eq!(s.rngs().len(), MAX_RNG_STREAMS, "capped");
    assert_eq!(s.store.get("refused"), Some(&Value::from("rng_full")));
    assert_eq!(s.rng_sum, rng_sum_from_scratch(s), "kept through draws and failed calls");
    let bytes = a.host.save_state().expect("a module ran");
    let mut b = Run { host: LogicHost::new(), world: a.world.clone(), tick: a.tick };
    b.load(ModuleCode::of::<StreamHog>()).unwrap();
    let notes = b.host.restore_state(&bytes).expect("a save at the cap loads");
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(b.host.shared["hog"].rngs(), a.host.shared["hog"].rngs());
    assert_eq!(b.host.state_hash(), a.host.state_hash());
    for t in 0..10 {
        a.ticks(1);
        b.ticks(1);
        assert_eq!(b.host.state_hash(), a.host.state_hash(), "diverged {t} ticks after the load");
    }
    // A draw changes the hash; undoing it puts the hash back.
    let s = a.host.shared.get_mut("hog").unwrap();
    let (before, sum) = (s.rngs().get("thing_0_0").copied(), s.rng_sum);
    s.draw("thing_0_0", || unreachable!("exists"), |r| r.next_u32());
    assert_ne!(s.rng_sum, sum);
    s.put_rng("thing_0_0".into(), before);
    assert_eq!(s.rng_sum, sum);
}

/// Modules can't write reserved data (`/lex`, `/lex_live`, `/project`, `/nav`): the request is
/// refused with `reserved` and undone; the module carries on.
#[derive(Default)]
struct Trespasser;
impl Module for Trespasser {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "trespasser", ..ModuleInfo::default() }
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        let mut codes = Vec::new();
        let tries = [
            Spawn::new("group").name("lex"),
            Spawn::new("cube").name("lex_live"),
            Spawn::new("cube").name("project"),
            Spawn::new("group").name("nav"),
            Spawn::new("group").name("rules").props(Value::map().with("lex", Value::map())),
            Spawn::new("group").name("mesh").props(Value::map().with(crate::nav::MESH_KEY, Value::map())),
        ];
        for s in tries {
            codes.push(Value::from(cx.spawn(s).err().map_or("ok".to_owned(), |e| e.code)));
        }
        // Turning an ordinary entity into rule data is refused too.
        let id = cx.spawn(Spawn::new("group").name("box"))?;
        let changed = cx.set(id, Patch::new().props(Value::map().with("lex", Value::map())));
        codes.push(Value::from(changed.err().map_or("ok".to_owned(), |e| e.code)));
        cx.set_state("codes", Value::List(codes))
    }
}

#[test]
fn modules_cannot_write_reserved_data() {
    let mut r = Run::new();
    r.world.set_recording(Recording::Full);
    r.load(ModuleCode::of::<Trespasser>()).unwrap();
    r.ticks(1);
    let codes = r.state("trespasser", "codes");
    let codes: Vec<&str> = codes.as_list().unwrap_or_default().iter().filter_map(Value::as_str).collect();
    assert_eq!(codes, ["reserved"; 7], "{codes:?}");
    for p in ["/lex", "/lex_live", "/project", "/nav", "/rules", "/mesh"] {
        assert!(r.world.find_path(p).is_err(), "{p} was undone");
    }
    let id = r.world.find_path("/box").expect("the module carried on");
    assert!(r.world.get(id).unwrap().props.get("lex").is_none(), "the refused change was undone");
}

/// Loads, reloads, unloads and disables become module events with their tick, generation and
/// build (the engine journals them as kind `module` and gives them to the desync detector).
#[test]
fn loads_reloads_unloads_and_disables_are_module_events() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    r.ticks(3);
    r.load(ModuleCode::of::<WalkerV2>()).unwrap();
    let evs = r.host.take_module_events();
    let v1 = ModuleCode::of::<WalkerV1>().build_hash();
    let v2 = ModuleCode::of::<WalkerV2>().build_hash();
    assert_eq!(evs.len(), 2);
    assert_eq!((evs[0].event, evs[0].tick, evs[0].generation, evs[0].build_hash), ("load", 0, 1, v1));
    assert_eq!((evs[1].event, evs[1].tick, evs[1].generation, evs[1].build_hash, evs[1].previous_build), ("reload", 3, 2, v2, Some(v1)));
    assert!(r.host.take_module_events().is_empty(), "taken once");
    assert_eq!(r.host.builds(), vec![("walker".to_owned(), v2)]);
    // A failed load is no event: nothing changed.
    assert!(r.load(ModuleCode::of::<BadName>()).is_err());
    assert!(r.host.take_module_events().is_empty());
    r.host.unload("walker", false, &mut r.world, r.tick, 30, &mut no_journal()).unwrap();
    let evs = r.host.take_module_events();
    assert_eq!((evs[0].event, evs[0].build_hash, evs[0].running()), ("unload", v2, false));
}

/// A module that changes the rounding mode: the environment is restored after every call, the
/// module's own maths start each call in the default one, and the change is counted against the
/// module (logic_status, perf_frame).
#[test]
fn a_module_that_changes_the_rounding_mode_is_restored_and_reported() {
    std::thread::spawn(|| {
        if !specula_sys::fpenv::SUPPORTED {
            eprintln!("skipped: no floating-point control register is checked on this target");
            return;
        }
        let mut r = Run::new();
        r.load(ModuleCode::of::<specula_logic::fixtures::FlipRounding>()).unwrap();
        r.ticks(5);
        assert_eq!(specula_sys::fpenv::control(), Some(specula_sys::fpenv::EXPECTED), "restored after the ticks");
        let log = r.host.fp_log();
        assert_eq!(log.count("module:fliprounding"), 5, "one change found per tick");
        let status = r.host.status(Some("fliprounding"), false).unwrap();
        let m = &status.get("modules").unwrap().as_array().unwrap()[0];
        assert_eq!(m.get("fp_restored").and_then(Json::as_u64), Some(5));
        assert_eq!(m.get("fp_last").and_then(|l| l.get("rounding")).and_then(Json::as_str), Some("toward_zero"));
        assert!(log.problem().is_some(), "perf_frame lists it");
        // The module's own maths start in the default environment each call.
        let third = r.host.call_hook("fliprounding/fp_probe", &Json::Null, "test", "t", &mut r.world, r.tick, 30, &mut no_journal()).unwrap();
        assert_eq!(third.as_u64(), Some(u64::from((1.0f32 / 3.0).to_bits())));
        assert_eq!(specula_sys::fpenv::control(), Some(specula_sys::fpenv::EXPECTED));
        assert_eq!(r.host.fp_log().count("module:fliprounding"), 6);
    })
    .join()
    .expect("test thread");
}

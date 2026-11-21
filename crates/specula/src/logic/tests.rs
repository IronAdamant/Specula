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
    r.host.hold("walker", &Json::parse(r#"{"move": [1, 0]}"#).unwrap(), 1000, None).unwrap();
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
    r.host.hold("walker", &Json::parse(r#"{"move": [1, 0]}"#).unwrap(), 1000, None).unwrap();
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
        r.host.hold("walker", &Json::parse(r#"{"move": [0.7, 0], "jump": true}"#).unwrap(), 17, None).unwrap();
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

/// What a hosted game is lent of its modules' state is the running modules' alone (review R1
/// of M6 part 3: a module unloaded and not forgotten left its last run's count behind, and a
/// new run was won on it at once; review R2: no test held the fix).
#[test]
fn a_game_is_lent_only_running_modules_state() {
    use crate::game::ModuleStates;
    let mut r = Run::new();
    r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    r.ticks(3);
    {
        let lent = ModuleStates::of(&r.host);
        assert_eq!(lent.get("walker", "ticks"), Some(&Value::Int(3)));
        assert_eq!(lent.dotted("walker.ticks"), Some(&Value::Int(3)));
        assert_eq!(lent.each().map(|(name, _)| name).collect::<Vec<_>>(), ["walker"]);
        assert_eq!(lent.running(), ["walker"]);
    }
    // Unloaded, its state kept for the next load: still in the host, and lent to nobody.
    r.host.unload("walker", false, &mut r.world, r.tick, 30, &mut no_journal()).unwrap();
    assert_eq!(r.state("walker", "ticks"), Value::Int(3), "the state is kept");
    let lent = ModuleStates::of(&r.host);
    assert_eq!(lent.get("walker", "ticks"), None);
    assert_eq!(lent.dotted("walker.ticks"), None);
    assert_eq!(lent.each().count(), 0);
    assert!(lent.running().is_empty());
    // Loaded again, it is lent again (the state it kept with it).
    r.load(ModuleCode::of::<WalkerV1>()).unwrap();
    assert_eq!(ModuleStates::of(&r.host).get("walker", "ticks"), Some(&Value::Int(3)));
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
    // Loading native code needs a granted lead (both are, so the claim is what refuses below).
    swarm.grants.grant("modder", crate::authority::Authority::Lead).unwrap();
    swarm.grants.grant("intruder", crate::authority::Authority::Lead).unwrap();
    swarm.hello(&so, "modder".into(), "lead".into(), None, &live).unwrap();
    swarm.hello(&si, "intruder".into(), "lead".into(), None, &live).unwrap();
    let e = swarm.hello(&session("sneaky03"), "module:walker2".into(), "builder".into(), None, &live).unwrap_err();
    assert!(matches!(e.code, "name_reserved" | "name_invalid"), "the whole module: prefix is refused, loaded or not");
    let (ko, ki) = (swarm.key(&so), swarm.key(&si));
    let (owner, intruder) = (Writer::new("modder", Some(&ko)), Writer::new("intruder", Some(&ki)));
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
    a.host.hold("walker", &Json::parse(r#"{"move": [1, 0], "jump": true}"#).unwrap(), 25, None).unwrap();
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

/// Draws three numbers a call from one stream, and puts the stream back at its beginning before
/// its third lot. The call after that restarts it too and then fails (rolled back).
#[derive(Default)]
struct Again;
impl Module for Again {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "again", ..ModuleInfo::default() }
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        let lot = cx.state("lots")?.as_u64().unwrap_or(0);
        let failed = cx.tick() == 3;
        if lot == 2 || failed {
            cx.random_restart("columns")?;
        }
        if failed {
            return Err(SdkError::new("odd", "restarted, then failed"));
        }
        let mut drawn = Vec::new();
        for _ in 0..3 {
            drawn.push(Value::from(cx.random_below("columns", 1000)?));
        }
        cx.set_state(&format!("lot{lot}"), drawn)?;
        // (A stream nobody drew from is at its beginning: restarting it makes none.)
        cx.random_restart("never")?;
        cx.set_state("lots", lot + 1)
    }
}

/// A stream put back at its beginning gives its first numbers again (a run that starts again
/// "with the same seed": seven of eight one-screen builds of the comparison wrote a generator of
/// their own for want of this), a restart in a call that fails is undone with the call, and the streams'
/// kept hash follows both.
#[test]
fn a_random_stream_begins_again_when_asked_and_a_failed_call_undoes_it() {
    let mut a = Run::new();
    a.load(ModuleCode::of::<Again>()).unwrap();
    // Ticks 0, 1, 2 draw lots 0, 1, 2 (the third after a restart); tick 3 restarts and fails;
    // tick 4 draws lot 3.
    a.ticks(5);
    let s = &a.host.shared["again"];
    let lot = |n: u64| s.store.get(&format!("lot{n}")).cloned().unwrap_or(Value::Null);
    assert!(lot(0).as_list().is_some_and(|l| l.len() == 3), "{:?}", lot(0));
    assert_ne!(lot(1), lot(0), "a stream goes on from where it was");
    assert_eq!(lot(2), lot(0), "restarted: the first numbers again");
    assert_eq!(lot(3), lot(1), "the restart of the call that failed was undone: the stream went on after its third lot");
    assert_eq!(s.store.get("lots"), Some(&Value::from(4u64)));
    assert!(!s.rngs().contains_key("never"), "restarting a stream that was never drawn from makes no stream");
    assert_eq!(s.rng_sum, rng_sum_from_scratch(s), "kept through restarts and the failed call");
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

/// Review of the gmods lane (lead direction 1): loading, replacing and unloading a module is
/// native code with the owner's user rights, so a builder (one that asked for lead included) is
/// refused (lead_only) on the engine thread itself, whatever the caller thread checked; a lead
/// the operator granted, and the owner, may. A builder's build becomes a proposal that the lead
/// approves only while the folder still has the proposed digest.
#[test]
fn only_the_owner_or_a_granted_lead_loads_native_code() {
    use crate::journal::Writer;
    use crate::swarm::Swarm;
    use specula_protocol::{Content, SessionInfo, ToolOutput};
    let session = |id: &str| SessionInfo { id: id.into(), client_name: "t".into(), client_version: "1".into(), protocol_version: "x".into() };
    let text = |o: &ToolOutput| match &o.content[0] {
        Content::Text(t) => t.clone(),
        _ => String::new(),
    };
    let code = |o: &ToolOutput| Json::parse(&text(o)).ok().and_then(|j| j.get("error").and_then(Json::as_str).map(str::to_owned)).unwrap_or_default();
    let load = || tools::LogicCall::Load { code: ModuleCode::of::<WalkerV1>(), origin: Origin::default(), seed: None, open_ms: 0.0 };
    let clock = crate::game::Clock::new(30);
    let mut world = World::default();
    world.set_recording(Recording::Full);
    // Refused tries go to the security monitor (global: one test at a time).
    let _g = crate::inputguard::TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    crate::inputguard::reset_for_test();

    // No grants at all: nobody leads. Saying lead gets nothing (owner, 2026-11-12), and the
    // agent that tried is a builder like any other.
    let mut open = Swarm::default();
    let (sf, sb) = (session("fallb001"), session("build002"));
    let live = vec![sf.clone(), sb.clone()];
    assert_eq!(open.hello(&sf, "first".into(), "lead".into(), None, &live).unwrap_err().code, "role_not_granted");
    open.hello(&sf, "first".into(), "builder".into(), None, &live).unwrap();
    open.hello(&sb, "ann".into(), "builder".into(), None, &live).unwrap();
    let (kf, kb) = (open.key(&sf), open.key(&sb));
    let mut host = LogicHost::new();
    for (who, key) in [("first", &kf), ("ann", &kb)] {
        let out = tools::handle(&mut host, load(), &mut world, &clock, &mut open, Writer::new(who, Some(key)));
        assert!(out.is_error && code(&out) == "lead_only", "{who}: {}", text(&out));
        assert!(text(&out).contains("logic.native_code"), "refused_at names the site: {}", text(&out));
        let gate = tools::handle(&mut host, tools::LogicCall::Gate(tools::Doing::Reload), &mut world, &clock, &mut open, Writer::new(who, Some(key)));
        assert_eq!(code(&gate), "lead_only", "{who} can't load a library either");
    }
    assert!(host.modules().is_empty(), "nothing loaded");
    // The monitor saw both tries on its native_code channel: each is an incident that freezes
    // the identity that made it.
    assert!(crate::inputguard::identity_frozen(&kb), "a builder's refused library load freezes it");
    assert!(crate::inputguard::identity_frozen(&kf), "and the one that had asked for lead");
    crate::inputguard::reset_for_test();

    // A granted lead loads; a builder can't unload it, nor replace it.
    let mut sw = Swarm::default();
    sw.grants.grant("lee", crate::authority::Authority::Lead).unwrap();
    let (sl, sa) = (session("lead0001"), session("ann00002"));
    let live = vec![sl.clone(), sa.clone()];
    sw.hello(&sl, "lee".into(), "lead".into(), None, &live).unwrap();
    sw.hello(&sa, "ann".into(), "builder".into(), None, &live).unwrap();
    let (kl, ka) = (sw.key(&sl), sw.key(&sa));
    let (lee, ann) = (Writer::new("lee", Some(&kl)), Writer::new("ann", Some(&ka)));
    let out = tools::handle(&mut host, load(), &mut world, &clock, &mut sw, lee);
    assert!(!out.is_error, "{}", text(&out));
    let out = tools::handle(&mut host, tools::LogicCall::Unload { module: "walker".into(), forget: false }, &mut world, &clock, &mut sw, ann);
    assert_eq!(code(&out), "lead_only", "{}", text(&out));
    assert_eq!(host.modules(), vec!["walker".to_owned()], "still loaded");
    assert!(crate::inputguard::identity_frozen(&ka), "a builder's refused unload freezes it");
    crate::inputguard::reset_for_test();

    // ann proposes a module: nothing builds; lee approves it only with the proposed digest.
    let digest = |hex: &str| Ok(specula_logic::digest::Digest { hex: hex.into(), files: 3, bytes: 99 });
    let dir = std::path::PathBuf::from("games/anns");
    let build = |digest, approve| tools::LogicCall::Gate(tools::Doing::Build { dir: dir.clone(), digest, approve });
    let out = tools::handle(&mut host, build(digest("aa"), None), &mut world, &clock, &mut sw, ann);
    assert_eq!(code(&out), "lead_only", "{}", text(&out));
    let answer = Json::parse(&text(&out)).unwrap();
    let id = answer.get("proposal").and_then(|p| p.get("id")).and_then(Json::as_u64).expect("the refusal carries the proposal");
    assert!(answer.get("next").is_some(), "and says what happens next");
    assert!(!crate::inputguard::identity_frozen(&ka), "proposing is ordinary work: no incident");
    let out = tools::handle(&mut host, build(digest("aa"), Some(id)), &mut world, &clock, &mut sw, ann);
    assert_eq!(code(&out), "lead_only", "ann can't approve her own proposal");
    assert!(crate::inputguard::identity_frozen(&ka), "approving without authority is a try: an incident");
    let out = tools::handle(&mut host, build(digest("bb"), Some(id)), &mut world, &clock, &mut sw, lee);
    assert_eq!(code(&out), "sources_changed", "the folder changed since the proposal: {}", text(&out));
    let out = tools::handle(&mut host, build(digest("aa"), Some(id)), &mut world, &clock, &mut sw, lee);
    assert!(!out.is_error, "{}", text(&out));
    assert_eq!(Json::parse(&text(&out)).unwrap().get("approved").and_then(|a| a.get("by")).and_then(Json::as_str), Some("ann"));
    assert_eq!(host.proposals.len(), 0, "approved proposals are taken");
    // A builder whose folder can't be digested gets no proposal, and says why.
    let out = tools::handle(&mut host, build(Err(("too_big", "too many files".into())), None), &mut world, &clock, &mut sw, ann);
    assert!(code(&out) == "lead_only" && text(&out).contains("proposal_refused"), "{}", text(&out));
    assert_eq!(host.proposals.len(), 0);
    crate::inputguard::reset_for_test();
}

/// Spawns with the SDK's typed words for a turn, a collider, a body and a light.
#[derive(Default)]
struct TypedSpawns;
impl Module for TypedSpawns {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "typed", ..ModuleInfo::default() }
    }
    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        use specula_sdk::Vec3;
        cx.spawn(Spawn::new("cube").name("ramp").rotation_deg(Vec3::new(0.0, 0.0, -20.0)).collider_box(Vec3::new(1.0, 1.0, 1.0)))?;
        cx.spawn(Spawn::new("sphere").name("ball").collider_sphere(0.5).dynamic(2.0))?;
        cx.spawn(Spawn::new("cube").name("lift").collider_box(Vec3::new(1.0, 1.0, 1.0)).kinematic())?;
        cx.spawn(Spawn::new("sphere").name("pill").collider_capsule(0.25, 1.0).dynamic(1.0))?;
        cx.spawn(Spawn::new("group").name("lamp").light(6.0, 12.0))?;
        Ok(())
    }
}

/// The SDK's typed words for a spawn are fields the engine takes, and mean what they say
/// (they are sugar over `field`, which the compiler cannot check: this test is the check).
#[test]
fn the_sdks_typed_spawn_words_are_what_the_engine_takes() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<TypedSpawns>()).unwrap();
    let of = |r: &Run, path: &str| r.world.get(r.world.find_path(path).unwrap_or_else(|e| panic!("{path}: {e:?}"))).unwrap().clone();
    let ramp = of(&r, "/ramp");
    assert!((ramp.rotation_deg.z + 20.0).abs() < 1e-5 && ramp.collider.is_some() && ramp.body.is_none(), "a turned collider that stands still");
    let ball = of(&r, "/ball");
    assert!(format!("{:?}", ball.collider).contains("Sphere") && format!("{:?}", ball.body).contains("Dynamic") && format!("{:?}", ball.body).contains("2.0"), "{:?} {:?}", ball.collider, ball.body);
    assert!(format!("{:?}", of(&r, "/lift").body).contains("Kinematic"));
    assert!(format!("{:?}", of(&r, "/pill").collider).contains("Capsule"));
    let lamp = of(&r, "/lamp").light.expect("a light");
    assert!(format!("{lamp:?}").contains("6.0") && format!("{lamp:?}").contains("12.0"), "{lamp:?}");
    // A model by its recipe is a spawn of shape `model` with that recipe (building one needs
    // a model library, which this host has not: the words are held as they are written).
    assert_eq!(Spawn::model("models/gear.json").0, specula_sdk::Value::map().with("shape", "model").with("recipe", "models/gear.json"));
}

/// Pushes a spinning ball, then puts it at rest.
#[derive(Default)]
struct Pusher;
impl Module for Pusher {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "pusher", ..ModuleInfo::default() }
    }
    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        use specula_sdk::{Value, Vec3};
        let spinning = Value::map().with("kind", "dynamic").with("mass", 2.0).with("velocity", Vec3::new(1.0, 0.0, 0.0)).with("angular_velocity", Vec3::new(0.0, 0.0, -4.0));
        cx.spawn(Spawn::new("sphere").name("ball").collider_sphere(0.5).bounce(0.5).friction(0.25).field("body", spinning))?;
        cx.spawn(Spawn::new("cube").name("wall").collider_box(Vec3::new(1.0, 1.0, 1.0)))?;
        Ok(())
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        use specula_sdk::Vec3;
        let ball = cx.find("/ball")?.expect("the ball");
        match cx.tick() {
            1 => cx.push(ball, Vec3::new(0.5, 2.0, 0.0))?,
            2 => cx.place_at_rest(ball, Vec3::new(0.0, 3.0, 0.0))?,
            3 => {
                let wall = cx.find("/wall")?.expect("the wall");
                let refused = cx.push(wall, Vec3::new(1.0, 0.0, 0.0)).unwrap_err();
                cx.set_state("refused", refused.code.as_str())?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// `Cx::push` adds to a body's velocity and keeps the rest of it (its spin, its mass);
/// `place_at_rest` moves it and takes its speed and spin away; a thing with no body is refused
/// by name; and the bounce and the friction go into the collider a collider word made.
#[test]
fn a_push_keeps_the_bodys_spin_and_a_placing_takes_it() {
    let mut r = Run::new();
    r.load(ModuleCode::of::<Pusher>()).unwrap();
    let ball = |r: &Run| r.world.get(r.world.find_path("/ball").unwrap()).unwrap().clone();
    let c = ball(&r).collider.expect("a collider");
    assert!((c.restitution - 0.5).abs() < 1e-6 && (c.friction - 0.25).abs() < 1e-6 && format!("{:?}", c.shape).contains("Sphere"), "{c:?}");
    r.ticks(2);
    let b = ball(&r).body.expect("a body");
    assert!((b.velocity.x - 1.5).abs() < 1e-5 && (b.velocity.y - 2.0).abs() < 1e-5, "the push is added: {:?}", b.velocity);
    assert!((b.angular_velocity.z + 4.0).abs() < 1e-5 && (b.mass - 2.0).abs() < 1e-6, "its spin and its mass are kept: {b:?}");
    r.ticks(1);
    let (e, b) = (ball(&r), ball(&r).body.unwrap());
    assert!((e.position.y - 3.0).abs() < 1e-6 && b.velocity.length() < 1e-6 && b.angular_velocity.length() < 1e-6 && (b.mass - 2.0).abs() < 1e-6, "{e:?}");
    r.ticks(1);
    assert_eq!(r.state("pusher", "refused").as_str(), Some("no_body"));
}

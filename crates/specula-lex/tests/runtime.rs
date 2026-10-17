//! The runtime: each building block behaves as documented, deterministically.

use std::sync::Arc;

use specula_core::Json;
use specula_lex::run::{compile_query, run, RunSpec, Timed};
use specula_lex::sim::{BehState, NativeCtx};
use specula_lex::{compile, Document, NativeHook, Natives, Options, Sim};

fn rules(items: &str) -> Arc<specula_lex::Rules> {
    let d = Document::parse(&format!(r#"{{"lex": 1, "name": "t", "tick_hz": 10, "items": {{{items}}}}}"#)).unwrap_or_else(|d| panic!("{d:?}"));
    match compile(&d, &Options::default()) {
        Ok(c) => {
            let errors: Vec<String> = c.warnings.iter().map(|w| w.to_string()).collect();
            assert!(errors.is_empty(), "warnings: {errors:#?}");
            Arc::new(c.rules)
        }
        Err(ds) => panic!("{}", ds.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("\n")),
    }
}

fn sim(items: &str, seed: u64) -> Sim {
    Sim::new(rules(items), seed, &Natives::new()).unwrap()
}

fn cmd(s: &mut Sim, name: &str, args: &str) -> Result<(), specula_lex::Rejection> {
    let call = s.parse_command(name, &Json::parse(args).unwrap())?;
    s.apply(&call)
}

/// A dummy target with health and a speed scale, spawned by a command.
const BASE: &str = r#"
    "gold": {"kind": "resource", "start": 0},
    "spots": {"kind": "sites", "points": {"a": [0, 0, 0], "b": [10, 0, 0]}},
    "dummy": {"kind": "archetype", "tags": ["enemy"],
              "attributes": {"hp": "100pt", "speed_scale": "100%", "armor": "0pt"},
              "behaviours": [{"type": "health", "attribute": "hp", "on_death": [{"add": "gold", "amount": 1}]}]},
    "make": {"kind": "command", "args": {"what": "archetype", "at": "site"}, "do": [{"spawn": "arg.what", "at": "arg.at"}]}
"#;

fn with_base(extra: &str) -> String {
    format!("{BASE}, {extra}")
}

fn first_id(s: &Sim) -> u32 {
    s.entities()[0].id
}

#[test]
fn effects_stack_by_policy() {
    let fx = |stacking: &str| {
        with_base(&format!(
            r#""slow": {{"kind": "effect", "duration": "1s", "stacking": "{stacking}", "max_stacks": 3,
                        "modifiers": [{{"attribute": "speed_scale", "op": "mul", "value": "100% - magnitude"}}]}},
               "hit": {{"kind": "command", "args": {{"m": "ratio"}}, "do": [{{"apply": "slow", "to": "occupant", "magnitude": "arg.m"}}]}}"#
        ))
        .replace("\"to\": \"occupant\"", "\"to\": \"arg.t\"")
        .replace(r#""args": {"m": "ratio"}"#, r#""args": {"m": "ratio", "t": "entity"}"#)
    };
    let setup = |stacking: &str| {
        let mut s = sim(&fx(stacking), 1);
        cmd(&mut s, "make", r#"{"what": "dummy", "at": "a"}"#).unwrap();
        s
    };
    let scale = |s: &Sim| s.attr(first_id(s), "speed_scale").unwrap();
    let hit = |s: &mut Sim, m: f64| cmd(s, "hit", &format!(r#"{{"m": {m}, "t": "a"}}"#)).unwrap();

    // strongest: a weaker slow is ignored, a stronger one replaces it.
    let mut s = setup("strongest");
    hit(&mut s, 0.35);
    assert_eq!(scale(&s), 650);
    hit(&mut s, 0.2);
    assert_eq!(scale(&s), 650, "weaker is ignored");
    hit(&mut s, 0.5);
    assert_eq!(scale(&s), 500);
    for _ in 0..10 {
        s.step(&[]);
    }
    assert_eq!(scale(&s), 1000, "expired after 1 s");

    // stack: modifiers apply once per stack, up to max_stacks.
    let mut s = setup("stack");
    for _ in 0..5 {
        hit(&mut s, 0.1);
    }
    assert_eq!(scale(&s), 729, "0.9^3, truncating each time");

    // independent: separate instances up to max_stacks, the oldest replaced.
    let mut s = setup("independent");
    for m in [0.1, 0.2, 0.3, 0.4] {
        hit(&mut s, m);
    }
    assert_eq!(s.entity(first_id(&s)).unwrap().effects.len(), 3);
    assert_eq!(scale(&s), 800 * 700 / 1000 * 600 / 1000, "0.1 dropped");

    // ignore: a running instance blocks new ones.
    let mut s = setup("ignore");
    hit(&mut s, 0.1);
    hit(&mut s, 0.9);
    assert_eq!(scale(&s), 900);

    // refresh: the magnitude and the timer restart.
    let mut s = setup("refresh");
    hit(&mut s, 0.5);
    for _ in 0..8 {
        s.step(&[]);
    }
    hit(&mut s, 0.1);
    for _ in 0..8 {
        s.step(&[]);
    }
    assert_eq!(scale(&s), 900, "still active: the timer restarted");
}

#[test]
fn periodic_effects_tick_base_values_and_kill() {
    let items = with_base(
        r#""burn": {"kind": "effect", "duration": "infinite", "period": "0.5s", "magnitude": "points",
                    "modifiers": [{"attribute": "hp", "op": "add", "value": "-magnitude"}]},
           "ignite": {"kind": "command", "args": {"t": "entity"}, "do": [{"apply": "burn", "to": "arg.t", "magnitude": "30pt"}]},
           "deaths": {"kind": "resource", "start": 0},
           "count_deaths": {"kind": "rule", "on": "died", "tags": ["enemy"], "do": [{"add": "deaths", "amount": 1}]}"#,
    );
    let mut s = sim(&items, 1);
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "a"}"#).unwrap();
    cmd(&mut s, "ignite", r#"{"t": "a"}"#).unwrap();
    let id = first_id(&s);
    for _ in 0..5 {
        s.step(&[]);
    }
    assert_eq!(s.attr(id, "hp"), Some(100_000), "the first period ends at tick 5");
    s.step(&[]);
    assert_eq!(s.attr(id, "hp"), Some(70_000), "one period after 0.5 s");
    for _ in 0..16 {
        s.step(&[]);
    }
    assert!(s.entity(id).is_none(), "dead and removed");
    assert_eq!((s.resource("gold"), s.resource("deaths")), (Some(1), Some(1)), "on_death and the died rule both ran");
}

#[test]
fn block_and_grant_tags() {
    let items = with_base(
        r#""ward": {"kind": "effect", "duration": "infinite", "grant_tags": ["warded"]},
           "slow": {"kind": "effect", "duration": "1s", "block_tags": ["warded"],
                    "modifiers": [{"attribute": "speed_scale", "op": "mul", "value": "50%"}]},
           "go": {"kind": "command", "args": {"t": "entity", "e": "int"},
                  "do": [{"if": "arg.e == 0", "then": [{"apply": "ward", "to": "arg.t"}], "else": [{"apply": "slow", "to": "arg.t"}]}]},
           "warded_count": {"kind": "outcome", "result": "warded", "when": "count(warded) > 0 && false"}"#,
    );
    let mut s = sim(&items, 1);
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "a"}"#).unwrap();
    cmd(&mut s, "go", r#"{"t": "a", "e": 0}"#).unwrap();
    cmd(&mut s, "go", r#"{"t": "a", "e": 1}"#).unwrap();
    assert_eq!(s.attr(first_id(&s), "speed_scale"), Some(1000), "immune while warded");
    let q = compile_query(s.rules(), "count(warded)").unwrap();
    assert_eq!(s.query(&q.ex).unwrap(), specula_lex::Val::Int(1));
}

#[test]
fn timers_fire_on_schedule() {
    let mut s = sim(
        r#""gold": {"kind": "resource", "start": 0},
           "pay": {"kind": "timer", "start": "1s", "every": "0.5s", "count": 3, "when": "tick < 25t", "do": [{"add": "gold", "amount": 10}]}"#,
        1,
    );
    let mut seen = Vec::new();
    for _ in 0..40 {
        s.step(&[]);
        seen.push(s.resource("gold").unwrap());
    }
    // seen[i] is the state after tick i ran.
    assert_eq!((seen[9], seen[10]), (0, 10), "fires at tick 10");
    assert_eq!((seen[14], seen[15]), (10, 20), "and every 5 ticks");
    assert_eq!(seen[20], 30);
    assert_eq!(seen[39], 30, "count 3");
}

#[test]
fn machines_switch_behaviours_and_rules_hear_custom_events() {
    let items = with_base(
        r#""loop": {"kind": "path", "points": [[0,0,0], [4,0,0]], "loop": true},
           "guard_brain": {"kind": "machine", "initial": "patrol", "states": {
               "patrol": {"behaviours": ["walk"], "transitions": [{"to": "rest", "after": "2s"}]},
               "rest": {"behaviours": [], "on_enter": [{"emit": "tired"}], "transitions": [{"to": "patrol", "after": "1s"}]}}},
           "guard": {"kind": "archetype", "machine": "guard_brain", "attributes": {"speed": "1m/s"},
                     "behaviours": [{"type": "patrol", "name": "walk", "path": "loop", "speed": "self.speed"}]},
           "rests": {"kind": "resource", "start": 0},
           "note_rest": {"kind": "rule", "on": "tired", "do": [{"add": "rests", "amount": 1}]}"#,
    );
    let mut s = sim(&items, 1);
    cmd(&mut s, "make", r#"{"what": "guard", "at": "a"}"#).unwrap();
    let id = first_id(&s);
    // Transitions are checked after movement: tick 20 still walks, then rests.
    for _ in 0..21 {
        s.step(&[]);
    }
    let x_at_rest = s.entity(id).unwrap().pos[0];
    assert_eq!(x_at_rest, 2100, "walked 21 ticks at 0.1 m before resting");
    for _ in 0..5 {
        s.step(&[]);
    }
    assert_eq!(s.entity(id).unwrap().pos[0], x_at_rest, "the walk behaviour is off while resting");
    assert_eq!(s.resource("rests"), Some(1));
    for _ in 0..10 {
        s.step(&[]);
    }
    assert_ne!(s.entity(id).unwrap().pos[0], x_at_rest, "patrolling again");
}

#[test]
fn patrols_loop_and_pingpong() {
    let items = with_base(
        r#""line": {"kind": "path", "points": [[0,0,0], [2,0,0]]},
           "walker": {"kind": "archetype", "attributes": {"speed": "1m/s"},
                      "behaviours": [{"type": "patrol", "path": "line", "speed": "self.speed", "mode": "pingpong"}]}"#,
    );
    let mut s = sim(&items, 1);
    cmd(&mut s, "make", r#"{"what": "walker", "at": "a"}"#).unwrap();
    let id = first_id(&s);
    let xs: Vec<i64> = (0..50).map(|_| {
        s.step(&[]);
        s.entity(id).unwrap().pos[0]
    }).collect();
    assert_eq!(xs[19], 2000, "at the end after 2 s");
    assert_eq!(xs[29], 1000, "coming back");
    assert_eq!(xs[39], 0, "back at the start");
    assert_eq!(xs[49], 1000, "and out again");
}

#[test]
fn summons_continue_the_summoners_path_and_lifetimes_end() {
    let items = r#"
        "lane": {"kind": "path", "points": [[0,0,0], [100,0,0]]},
        "boss": {"kind": "archetype", "tags": ["boss"], "attributes": {"speed": "1m/s"},
                 "behaviours": [{"type": "follow_path", "path": "@spawn", "speed": "self.speed"},
                                {"type": "spawner", "archetype": "imp", "every": "3s", "count": 2}]},
        "imp": {"kind": "archetype", "tags": ["imp"], "attributes": {"speed": "1m/s"},
                "behaviours": [{"type": "follow_path", "path": "@spawn", "speed": "self.speed"}, {"type": "lifetime", "duration": "2s"}]},
        "waves": {"kind": "waves", "waves": [{"groups": [{"archetype": "boss", "count": 1, "at": ["lane"]}]}]}
    "#;
    let mut s = sim(items, 1);
    for _ in 0..31 {
        s.step(&[]);
    }
    let imps: Vec<_> = s.entities().iter().filter(|e| e.arch == s.rules().archetype("imp").unwrap()).collect();
    assert_eq!(imps.len(), 2, "two imps after 3 s");
    let boss = s.entities().iter().find(|e| e.arch == s.rules().archetype("boss").unwrap()).unwrap();
    for imp in &imps {
        assert_eq!(imp.wave, Some(1), "summons belong to the summoner's wave");
        assert!(matches!(imp.beh[0], BehState::Path { seg: 0, .. }));
        assert!((imp.pos[0] - boss.pos[0]).abs() <= 1000, "they start where the boss stands: {} vs {}", imp.pos[0], boss.pos[0]);
    }
    let imps = |s: &Sim| s.entities().iter().filter(|e| e.arch == s.rules().archetype("imp").unwrap()).count();
    for _ in 0..21 {
        s.step(&[]);
    }
    assert_eq!(imps(&s), 0, "the first pair is gone after 2 s");
    for _ in 0..10 {
        s.step(&[]);
    }
    assert_eq!(imps(&s), 2, "a new pair at 6 s");
}

#[test]
fn chase_reaches_and_acts() {
    let items = with_base(
        r#""hound": {"kind": "archetype", "attributes": {"speed": "2m/s"},
                     "behaviours": [{"type": "chase", "targets": ["enemy"], "range": "20m", "speed": "self.speed", "reach": "1m",
                                     "on_reach": [{"damage": "40pt"}]}]}"#,
    );
    let mut s = sim(&items, 1);
    cmd(&mut s, "make", r#"{"what": "hound", "at": "a"}"#).unwrap();
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "b"}"#).unwrap();
    let (hound, dummy) = (s.entities()[0].id, s.entities()[1].id);
    for _ in 0..50 {
        s.step(&[]);
    }
    assert_eq!(s.entity(hound).unwrap().pos[0], 9000, "stops at reach");
    assert_eq!(s.attr(dummy, "hp"), Some(60_000), "on_reach runs once per arrival");
}

#[test]
fn upgrades_keep_the_id_cooldown_and_kept_attributes() {
    let items = with_base(
        r#""t1": {"kind": "archetype", "attributes": {"cost": 5, "spent": 0}, "next": "t2",
                  "behaviours": [{"type": "attack", "targets": ["enemy"], "range": "20m", "cooldown": "10s", "delivery": "instant", "payload": [{"damage": "1pt"}]}]},
           "t2": {"kind": "archetype", "extends": "t1", "attributes": {"cost": 9}, "next": null},
           "up": {"kind": "command", "args": {"t": "entity"}, "require": [{"when": "exists(arg.t.next)", "code": "max_tier"}],
                  "do": [{"change": "arg.t.spent", "by": "arg.t.next.cost"}, {"become": "arg.t.next", "who": "arg.t", "keep": ["spent"]}]}"#,
    );
    let mut s = sim(&items, 1);
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "b"}"#).unwrap();
    cmd(&mut s, "make", r#"{"what": "t1", "at": "a"}"#).unwrap();
    s.step(&[]);
    let t = s.entities()[1].id;
    let ready = |s: &Sim| match s.entity(t).unwrap().beh[0] {
        BehState::Attack { ready_at } => ready_at,
        _ => panic!(),
    };
    assert_eq!(ready(&s), 100, "fired at tick 0");
    cmd(&mut s, "up", r#"{"t": "a"}"#).unwrap();
    let e = s.entity(t).unwrap();
    assert_eq!(e.arch, s.rules().archetype("t2").unwrap());
    assert_eq!((s.attr(t, "cost"), s.attr(t, "spent")), (Some(9), Some(9)));
    assert_eq!(ready(&s), 100, "the cooldown carries over");
    let e = cmd(&mut s, "up", r#"{"t": "a"}"#).unwrap_err();
    assert_eq!(e.code, "max_tier");
}

#[test]
fn commands_are_atomic_and_refusals_are_data() {
    let items = with_base(
        r#""buy": {"kind": "command", "args": {"what": "archetype", "at": "site"}, "cost": {"gold": 5},
                   "do": [{"spawn": "arg.what", "at": "arg.at"}]},
           "broken": {"kind": "command", "args": {"t": "entity"}, "do": [{"add": "gold", "amount": 100}, {"despawn": "arg.t"}, {"change": "arg.t.hp", "by": "1pt"}, {"despawn": "other_missing"}]},
           "win": {"kind": "outcome", "result": "won", "when": "res.gold >= 1000"},
           "rich": {"kind": "command", "do": [{"add": "gold", "amount": 1000}]}"#,
    );
    let items = items.replace(r#", {"despawn": "other_missing"}"#, "");
    let mut s = sim(&items, 1);
    let e = cmd(&mut s, "buy", r#"{"what": "dummy", "at": "a"}"#).unwrap_err();
    assert_eq!(e.code, "not_enough_gold");
    assert_eq!(e.to_json().get("need").and_then(Json::as_i64), Some(5));
    assert_eq!(cmd(&mut s, "buy", r#"{"what": "dumy", "at": "a"}"#).unwrap_err().code, "unknown_archetype");
    assert_eq!(cmd(&mut s, "buy", r#"{"what": "dummy", "at": "c"}"#).unwrap_err().code, "unknown_site");
    assert_eq!(cmd(&mut s, "buy", r#"{"what": "dummy"}"#).unwrap_err().code, "missing_arg");
    assert_eq!(cmd(&mut s, "nope", "{}").unwrap_err().code, "unknown_command");
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "a"}"#).unwrap();
    let before = s.hash();
    let e = cmd(&mut s, "broken", r#"{"t": "a"}"#).unwrap_err();
    assert_eq!(e.code, "runtime_error", "{e:?}");
    assert_eq!(s.hash(), before, "rolled back: gold and the entity are as before");
    assert_eq!(s.resource("gold"), Some(0));
    cmd(&mut s, "rich", "{}").unwrap();
    assert_eq!(s.outcome(), Some(("won", 0)));
    assert_eq!(cmd(&mut s, "rich", "{}").unwrap_err().code, "game_over");
}

struct Heal;

impl NativeHook for Heal {
    fn run(&self, cx: &mut NativeCtx<'_, '_>) -> Result<(), String> {
        let me = cx.entity().ok_or("no entity")?;
        let hp = cx.attr(me, "hp").ok_or("no hp")?;
        let by = cx.arg("by").unwrap_or(0);
        cx.set_attr(me, "hp", hp + by)?;
        let roll = cx.rand("loot", 1, 6)?;
        cx.add_resource("gold", i64::from(roll))
    }
}

#[test]
fn native_hooks_are_declared_bound_and_counted() {
    let items = with_base(
        r#""regen": {"kind": "native", "hook": "test.regen", "phase": "effects", "reason": "no regeneration block yet"},
           "troll": {"kind": "archetype", "tags": ["enemy"], "attributes": {"hp": "10pt", "luck": 0},
                     "behaviours": [{"type": "health", "attribute": "hp"}, {"type": "native", "hook": "regen", "args": {"by": "1pt"}}],
                     "on_spawn": [{"change": "self.luck", "to": "rand(loot, 0, 0)"}]}"#,
    );
    let r = rules(&items);
    let err = Sim::new(r.clone(), 1, &Natives::new()).unwrap_err();
    assert!(err.contains("test.regen") && err.contains("not provided"), "{err}");
    let mut natives = Natives::new();
    natives.insert("test.regen".into(), Arc::new(Heal) as Arc<dyn NativeHook>);
    let mut s = Sim::new(r, 1, &natives).unwrap();
    cmd(&mut s, "make", r#"{"what": "troll", "at": "a"}"#).unwrap();
    for _ in 0..3 {
        s.step(&[]);
    }
    let id = first_id(&s);
    assert_eq!(s.attr(id, "hp"), Some(13_000));
    assert_eq!(s.state().native_calls, vec![3]);
    assert!((3..=18).contains(&s.resource("gold").unwrap()));
}

#[test]
fn runtime_problems_are_reported_not_fatal() {
    let items = r#"
        "gold": {"kind": "resource", "start": 0},
        "z": {"kind": "param", "value": 0},
        "t": {"kind": "timer", "every": "1s", "do": [{"add": "gold", "amount": "10 / $z"}]},
        "echo": {"kind": "rule", "on": "ping", "do": [{"emit": "ping"}]},
        "start": {"kind": "timer", "start": "0.5s", "do": [{"emit": "ping"}]}
    "#;
    // A constant division by zero is a compile error...
    let d = Document::parse(&format!(r#"{{"lex": 1, "tick_hz": 10, "items": {{{items}}}}}"#)).unwrap();
    let ds = compile(&d, &Options::default()).unwrap_err();
    assert!(ds.iter().any(|x| x.message.contains("division by zero")), "{ds:?}");
    // ...a run-time one gives 0 and is counted; an event loop is cut off.
    let items = items.replace("10 / $z", "10 / (res.gold - res.gold)");
    let mut s = sim(&items, 1);
    for _ in 0..20 {
        s.step(&[]);
    }
    let diag = &s.state().diag;
    assert!(diag.counts.get("division_by_zero").is_some_and(|n| *n >= 2), "{diag:?}");
    assert_eq!(diag.counts.get("event_storm"), Some(&1), "{diag:?}");
    assert_eq!(s.resource("gold"), Some(0));
    assert_eq!(s.tick(), 20, "the run went on");
}

#[test]
fn runs_are_deterministic_and_reports_are_bounded() {
    let items = with_base(
        r#""spawn_dummy": {"kind": "timer", "every": "0.3s", "do": [{"spawn": "dummy", "at": "a"}]},
           "zap": {"kind": "timer", "every": "0.2s", "do": [{"if": "chance(zaps, 40%)", "then": [{"add": "gold", "amount": "rand(zaps, 1, 9)"}]}]}"#,
    );
    let r = rules(&items);
    let report = |seed: u64| {
        let mut s = Sim::new(r.clone(), seed, &Natives::new()).unwrap();
        let spec = RunSpec {
            ticks: 5000,
            commands: vec![Timed { tick: 3, name: "make".into(), call: s.parse_command("make", &Json::parse(r#"{"what": "dummy", "at": "b"}"#).unwrap()).unwrap() }],
            watch: vec![compile_query(&r, "res.gold").unwrap(), compile_query(&r, "count(enemy)").unwrap()],
            expect: vec![compile_query(&r, "res.gold > 0").unwrap(), compile_query(&r, "count(enemy) == 0").unwrap()],
            sample_every: 1,
        };
        run(&mut s, &spec)
    };
    let a = report(11);
    assert_eq!(a.to_string(), report(11).to_string(), "same seed, same report");
    assert_ne!(a.get("hash"), report(12).get("hash"));
    let samples = a.get("samples").and_then(Json::as_array).unwrap();
    assert!(samples.len() <= specula_lex::run::MAX_SAMPLES + 1, "{}", samples.len());
    let expect = a.get("expect").and_then(Json::as_array).unwrap();
    assert_eq!(expect[0].get("pass"), Some(&Json::Bool(true)));
    assert_eq!(expect[1].get("pass"), Some(&Json::Bool(false)));
    assert_eq!(a.get("expect_passed"), Some(&Json::Bool(false)));
    assert!(compile_query(&r, "rand(zaps, 1, 2) > 0").unwrap_err().message.contains("random"));
    assert_eq!(compile_query(&r, "self.hp").unwrap_err().code, "not_available");
}

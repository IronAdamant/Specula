//! The building blocks added for the whole-Sepulcra port (docs/design/LEX.md, "Grown from
//! Sepulcra"): data items, native binding, triggers, refusals and hosts, attack `when`,
//! phase `order`, group and spawner `with`, wave scale for summons, site groups, positions
//! in expressions, and the generation natives cache on.

use std::sync::{Arc, Mutex};

use specula_core::Json;
use specula_lex::rules::{NativeDef, Rules};
use specula_lex::sim::NativeHost;
use specula_lex::{compile, validate, Document, NativeCtx, NativeHook, Natives, Options, Sim};

fn doc(items: &str) -> Document {
    Document::parse(&format!(r#"{{"lex": 1, "name": "t", "tick_hz": 10, "items": {{{items}}}}}"#)).unwrap_or_else(|d| panic!("{d:?}"))
}

fn rules(items: &str) -> Arc<Rules> {
    match compile(&doc(items), &Options::default()) {
        Ok(c) => {
            let w: Vec<String> = c.warnings.iter().map(|w| w.to_string()).collect();
            assert!(w.is_empty(), "warnings: {w:#?}");
            Arc::new(c.rules)
        }
        Err(ds) => panic!("{}", ds.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("\n")),
    }
}

fn codes(items: &str) -> Vec<(String, String)> {
    validate(&doc(items), &Options::default()).iter().map(|d| (d.code.to_owned(), d.path.clone())).collect()
}

fn cmd(s: &mut Sim, name: &str, args: &str) -> Result<(), specula_lex::Rejection> {
    let call = s.parse_command(name, &Json::parse(args).unwrap())?;
    s.apply(&call)
}

const BASE: &str = r#"
    "gold": {"kind": "resource", "start": 0},
    "spots": {"kind": "sites", "points": {"a": [0, 0, 0], "b": [10, 0, 0], "c": [1, 2, 3]}},
    "lanes": {"kind": "sites", "points": {"l1": [0, 0, 5]}},
    "dummy": {"kind": "archetype", "tags": ["enemy"], "attributes": {"hp": "5pt", "face": 0, "mark": 0},
              "behaviours": [{"type": "health", "attribute": "hp", "on_death": [{"add": "gold", "amount": 1}]}]},
    "make": {"kind": "command", "args": {"what": "archetype", "at": "site"}, "do": [{"spawn": "arg.what", "at": "arg.at"}]}
"#;

fn with_base(extra: &str) -> String {
    format!("{BASE}, {extra}")
}

/// A hook that reads its data item at bind time and doubles `mark` by the table's factor; it
/// triggers `on.done` once mark passes 10 and refuses in commands when told to.
struct Doubler;

struct BoundDoubler {
    factor: i64,
    mark: u16,
}

impl NativeHook for Doubler {
    fn run(&self, _cx: &mut NativeCtx<'_, '_>) -> Result<(), String> {
        Err("unbound".into())
    }

    fn bind(&self, rules: &Rules, native: &NativeDef) -> Result<Option<Arc<dyn NativeHook>>, String> {
        let d = native.data.first().map(|i| &rules.data[usize::from(*i)]).ok_or("no data declared")?;
        let factor = d.value.get("factor").and_then(Json::as_i64).ok_or("the table needs a factor")?;
        let mark = rules.attr("mark").ok_or("no mark attribute")?;
        Ok(Some(Arc::new(BoundDoubler { factor, mark })))
    }
}

impl NativeHook for BoundDoubler {
    fn run(&self, cx: &mut NativeCtx<'_, '_>) -> Result<(), String> {
        if cx.arg("refuse") == Some(1) {
            return Err(cx.refuse("not_today", "the doubler refuses"));
        }
        let Some(me) = cx.entity() else { return Ok(()) };
        let m = cx.base(me, self.mark).unwrap_or(0).max(1) * self.factor;
        cx.set_many(me, &[(self.mark, m)])?;
        if m > 10 {
            cx.trigger("done")?;
        }
        Ok(())
    }
}

const DOUBLER: &str = r#"
    "table": {"kind": "data", "value": {"factor": 3, "notes": "data, not a note"}},
    "double": {"kind": "native", "hook": "test.double", "phase": "move", "data": ["table"], "reason": "a test"},
    "dbl": {"kind": "archetype", "tags": ["enemy"], "attributes": {"mark": 1},
            "behaviours": [{"type": "native", "hook": "double", "on": {"done": [{"add": "gold", "amount": 5}, {"despawn": "self"}]}}]},
    "nope": {"kind": "command", "do": [{"native": "double", "args": {"refuse": 1}}, {"add": "gold", "amount": 100}]}
"#;

fn doubler() -> Natives {
    let mut n = Natives::new();
    n.insert("test.double".into(), Arc::new(Doubler) as Arc<dyn NativeHook>);
    n
}

#[test]
fn natives_bind_to_data_trigger_their_action_lists_and_refuse_commands() {
    let r = rules(&with_base(DOUBLER));
    assert_eq!(r.data[0].value.get("factor"), Some(&Json::from(3)), "data items keep their JSON");
    let mut s = Sim::new(r.clone(), 1, &doubler()).expect("binds");
    cmd(&mut s, "make", r#"{"what": "dbl", "at": "a"}"#).unwrap();
    let id = s.entities()[0].id;
    s.step(&[]);
    assert_eq!(s.attr(id, "mark"), Some(3));
    s.step(&[]);
    assert_eq!(s.attr(id, "mark"), Some(9));
    s.step(&[]);
    assert_eq!(s.resource("gold"), Some(5), "on.done ran when mark passed 10");
    assert!(s.entities().is_empty(), "and despawned it");
    // A refusal names its own code, and nothing the command did is kept.
    let e = cmd(&mut s, "nope", "{}").unwrap_err();
    assert_eq!((e.code.as_str(), s.resource("gold")), ("not_today", Some(5)), "{e:?}");
    // Binding fails with the hook's reason when the table is wrong.
    let bad = rules(&with_base(&DOUBLER.replace(r#""factor": 3"#, r#""factor": "three""#)));
    let err = Sim::new(bad, 1, &doubler()).unwrap_err();
    assert!(err.contains("the table needs a factor"), "{err}");
}

#[test]
fn a_trigger_names_the_lists_it_has() {
    let items = with_base(&DOUBLER.replace(r#""on": {"done""#, r#""on": {"finished""#));
    let mut s = Sim::new(rules(&items), 1, &doubler()).unwrap();
    cmd(&mut s, "make", r#"{"what": "dbl", "at": "a"}"#).unwrap();
    for _ in 0..3 {
        s.step(&[]);
    }
    let first = s.state().diag.first.join("\n");
    assert!(first.contains("no 'on.done' action list (it has: finished)"), "{first}");
}

#[test]
fn natives_validate_their_streams_data_and_args() {
    let c = codes(&with_base(r#""n": {"kind": "native", "hook": "x.y", "reason": "r", "data": ["nothing"], "streams": ["Bad Name"]}"#));
    assert!(c.contains(&("unknown_ref".into(), "data[0]".into())), "{c:?}");
    assert!(c.contains(&("invalid".into(), "streams[0]".into())), "{c:?}");
    let args: Vec<String> = (0..17).map(|i| format!("\"a{i}\": {i}")).collect();
    let items = with_base(&format!(
        r#""n": {{"kind": "native", "hook": "x.y", "reason": "r"}}, "cmdn": {{"kind": "command", "do": [{{"native": "n", "args": {{{}}}}}]}}"#,
        args.join(", ")
    ));
    assert!(codes(&items).iter().any(|(c, p)| c == "too_many" && p == "do[0].args"), "{:?}", codes(&items));
    // x, y and z are positions, not attribute names.
    let c = codes(&with_base(r#""p": {"kind": "archetype", "attributes": {"x": 1}}"#));
    assert!(c.contains(&("invalid".into(), "attributes.x".into())), "{c:?}");
}

/// One host call: the hook, the entity, the args and the tick.
type Call = (String, Option<u32>, Vec<(String, i64)>, u32);

/// A host for the natives a run leaves to one: it records the calls.
#[derive(Default)]
struct Recorder {
    calls: Arc<Mutex<Vec<Call>>>,
    fail: bool,
}

impl NativeHost for Recorder {
    fn call(&mut self, hook: &NativeDef, entity: Option<u32>, args: specula_lex::NativeArgs<'_>, tick: u32) -> Result<(), String> {
        self.calls.lock().unwrap().push((hook.hook.clone(), entity, args.iter().map(|(k, v)| (k.to_owned(), v)).collect(), tick));
        if self.fail {
            Err("the module said no".into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn natives_without_an_implementation_go_to_the_host() {
    let items = with_base(
        r#""fx": {"kind": "native", "hook": "mod/fx", "phase": "move", "reason": "a world effect"},
           "glow": {"kind": "archetype", "tags": ["enemy"], "behaviours": [{"type": "native", "hook": "fx", "args": {"n": "2 + 3"}}]},
           "ping": {"kind": "command", "do": [{"native": "fx", "args": {"n": 7}}, {"add": "gold", "amount": 1}]}"#,
    );
    let r = rules(&items);
    assert!(Sim::new(r.clone(), 1, &Natives::new()).is_err(), "headless runs refuse unbound hooks");
    let mut s = Sim::new_hosted(r, 1, &Natives::new()).unwrap();
    assert_eq!(s.hosted_natives().iter().map(|n| n.hook.as_str()).collect::<Vec<_>>(), ["mod/fx"]);
    let mut host = Recorder::default();
    cmd(&mut s, "make", r#"{"what": "glow", "at": "a"}"#).unwrap();
    let id = s.entities()[0].id;
    s.step_hosted(&[], &mut host);
    let ping = s.parse_command("ping", &Json::Null).unwrap();
    s.apply_hosted(&ping, &mut host).unwrap();
    assert_eq!(s.resource("gold"), Some(1));
    let calls = host.calls.lock().unwrap().clone();
    assert_eq!(calls, vec![("mod/fx".into(), Some(id), vec![("n".into(), 5)], 0), ("mod/fx".into(), None, vec![("n".into(), 7)], 1)]);
    // A failing host call refuses the command (rolled back) and is a diagnostic in a phase.
    let mut failing = Recorder { fail: true, ..Default::default() };
    assert_eq!(s.apply_hosted(&ping, &mut failing).unwrap_err().code, "runtime_error");
    assert_eq!(s.resource("gold"), Some(1));
    s.step_hosted(&[], &mut failing);
    assert_eq!(s.state().diag.counts.get("native_failed"), Some(&1));
    // Without a host, the hosted natives fail like any run-time problem.
    s.step(&[]);
    assert_eq!(s.state().diag.counts.get("native_unbound"), Some(&1));
}

#[test]
fn attack_when_filters_candidates_but_splash_hits_everything_near() {
    let items = with_base(
        r#""bolt": {"kind": "archetype", "tags": ["projectile"], "behaviours": [{"type": "projectile", "speed": "100m/s", "homing": false, "splash": "3m"}]},
           "tower": {"kind": "archetype", "attributes": {"face": 1},
                     "behaviours": [{"type": "attack", "targets": ["enemy"], "range": "20m", "cooldown": "10s",
                                     "when": "target.face == self.face", "projectile": {"archetype": "bolt"}, "payload": [{"damage": "1pt"}]}]},
           "aim": {"kind": "command", "args": {"t": "entity", "f": "int"}, "do": [{"change": "arg.t.face", "to": "arg.f"}]}"#,
    );
    let mut s = Sim::new(rules(&items), 1, &Natives::new()).unwrap();
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "b"}"#).unwrap();
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "c"}"#).unwrap();
    let (far, near) = (s.entities()[0].id, s.entities()[1].id);
    cmd(&mut s, "make", r#"{"what": "tower", "at": "a"}"#).unwrap();
    for _ in 0..5 {
        s.step(&[]);
    }
    assert!(!s.entities().iter().any(|e| e.arch == s.rules().archetype("bolt").unwrap()), "nothing on the tower's face: no shot");
    cmd(&mut s, "aim", &format!(r#"{{"t": {far}, "f": 1}}"#)).unwrap();
    for _ in 0..5 {
        s.step(&[]);
    }
    assert_eq!(s.attr(far, "hp"), Some(4000), "the eligible target is hit");
    assert_eq!(s.attr(near, "hp"), Some(5000), "the other is out of the splash");
    // Splash ignores `when`: put the other one next to the target.
    let items2 = items.replace(r#""c": [1, 2, 3]"#, r#""c": [10, 0, 1]"#);
    let mut s = Sim::new(rules(&items2), 1, &Natives::new()).unwrap();
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "b"}"#).unwrap();
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "c"}"#).unwrap();
    let (t, side) = (s.entities()[0].id, s.entities()[1].id);
    cmd(&mut s, "aim", &format!(r#"{{"t": {t}, "f": 1}}"#)).unwrap();
    cmd(&mut s, "make", r#"{"what": "tower", "at": "a"}"#).unwrap();
    for _ in 0..5 {
        s.step(&[]);
    }
    assert_eq!((s.attr(t, "hp"), s.attr(side, "hp")), (Some(4000), Some(4000)), "the splash hit both");
}

#[test]
fn positions_and_sites_have_x_y_z() {
    let items = with_base(
        r#""probe": {"kind": "command", "args": {"s": "site", "e": "entity"},
                     "require": [{"when": "arg.s.x == 1m && arg.s.y == 2m && arg.s.z == 3m", "code": "site_pos"},
                                 {"when": "arg.e.x == arg.s.x && arg.e.z == 3m", "code": "entity_pos"}],
                     "do": [{"add": "gold", "amount": 1}]}"#,
    );
    let mut s = Sim::new(rules(&items), 1, &Natives::new()).unwrap();
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "c"}"#).unwrap();
    cmd(&mut s, "probe", r#"{"s": "c", "e": "c"}"#).unwrap();
    assert_eq!(cmd(&mut s, "probe", r#"{"s": "a", "e": "c"}"#).unwrap_err().code, "site_pos");
    let e = compile(&doc(&with_base(r#""bad": {"kind": "command", "args": {"s": "site"}, "require": [{"when": "arg.s.w > 0m", "code": "x"}], "do": []}"#)), &Options::default()).unwrap_err();
    assert!(e.iter().any(|d| d.message.contains("a site has no member 'w'")), "{e:?}");
}

#[test]
fn site_args_can_be_restricted_to_one_sites_item() {
    let items = with_base(r#""build": {"kind": "command", "args": {"at": {"type": "site", "of": "spots"}}, "do": [{"spawn": "dummy", "at": "arg.at"}]}"#);
    let s = Sim::new(rules(&items), 1, &Natives::new()).unwrap();
    assert!(s.parse_command("build", &Json::parse(r#"{"at": "b"}"#).unwrap()).is_ok());
    let e = s.parse_command("build", &Json::parse(r#"{"at": "l1"}"#).unwrap()).unwrap_err();
    assert_eq!(e.code, "wrong_site", "{e:?}");
    let c = codes(&with_base(r#""build": {"kind": "command", "args": {"at": {"type": "site", "of": "nowhere"}}, "do": []}"#));
    assert!(c.iter().any(|(c, p)| c == "unknown_ref" && p == "args.at.of"), "{c:?}");
    let c = codes(&with_base(r#""build": {"kind": "command", "args": {"at": {"type": "site", "off": "spots"}}, "do": []}"#));
    assert!(c.iter().any(|(c, p)| c == "unknown_field" && p == "args.at.off"), "{c:?}");
}

#[test]
fn order_decides_who_acts_first_in_a_phase() {
    // Two spawners due on the same tick; the one with the lower order spawns first (lower id).
    let items = with_base(
        r#""mark_a": {"kind": "archetype", "tags": ["mark"]},
           "mark_b": {"kind": "archetype", "tags": ["mark"]},
           "sp_a": {"kind": "archetype", "attributes": {"rank": 2}, "order": "self.rank",
                    "behaviours": [{"type": "spawner", "archetype": "mark_a", "every": "1s"}]},
           "sp_b": {"kind": "archetype", "attributes": {"rank": 1}, "order": "self.rank",
                    "behaviours": [{"type": "spawner", "archetype": "mark_b", "every": "1s"}]}"#,
    );
    let r = rules(&items);
    assert!(r.ordered);
    let mut s = Sim::new(r.clone(), 1, &Natives::new()).unwrap();
    cmd(&mut s, "make", r#"{"what": "sp_a", "at": "a"}"#).unwrap();
    cmd(&mut s, "make", r#"{"what": "sp_b", "at": "b"}"#).unwrap();
    for _ in 0..2 {
        s.step(&[]);
    }
    for _ in 0..10 {
        s.step(&[]);
    }
    let firsts: Vec<&str> = s.entities().iter().skip(2).take(2).map(|e| r.archetypes[usize::from(e.arch)].id.as_str()).collect();
    assert_eq!(firsts, ["mark_b", "mark_a"], "sp_b (order 1) ran before sp_a (order 2) though it is younger");
}

#[test]
fn group_with_draws_per_spawn_in_name_order_and_summons_join_their_waves_scale() {
    let items = |with: &str| {
        with_base(&format!(
            r#""kid": {{"kind": "archetype", "tags": ["enemy"], "attributes": {{"hp": "10pt", "from": 0}}, "behaviours": [{{"type": "health", "attribute": "hp"}}]}},
               "boss": {{"kind": "archetype", "tags": ["enemy"], "attributes": {{"hp": "10pt", "aa": 0, "zz": 0, "cell": 7}},
                        "behaviours": [{{"type": "health", "attribute": "hp"}},
                                       {{"type": "spawner", "archetype": "kid", "every": "1s", "count": 2, "with": {{"from": "self.cell"}}}}]}},
               "waves": {{"kind": "waves", "waves": [{{"scale": {{"hp": 2}}, "groups": [{{"archetype": "boss", "count": 3, "at": ["a", "b"], "every": "1s", "with": {with}}}]}}]}}"#
        ))
    };
    let a = items(r#"{"zz": "rand(waves, 1, 1000)", "aa": "rand(waves, 1, 1000)"}"#);
    let b = items(r#"{"aa": "rand(waves, 1, 1000)", "zz": "rand(waves, 1, 1000)"}"#);
    let run = |text: &str| {
        let mut s = Sim::new(rules(text), 9, &Natives::new()).unwrap();
        for _ in 0..25 {
            s.step(&[]);
        }
        s
    };
    let (sa, sb) = (run(&a), run(&b));
    assert_eq!(sa.hash(), sb.hash(), "the draws don't depend on key order");
    let r = sa.rules().clone();
    let bosses: Vec<_> = sa.entities().iter().filter(|e| e.arch == r.archetype("boss").unwrap()).collect();
    assert_eq!(bosses.len(), 3);
    let vals: Vec<(i64, i64)> = bosses.iter().map(|e| (sa.attr(e.id, "aa").unwrap(), sa.attr(e.id, "zz").unwrap())).collect();
    assert!(vals.iter().all(|(x, y)| (1..=1000).contains(x) && (1..=1000).contains(y)) && vals[0] != vals[1], "{vals:?}");
    let kids: Vec<_> = sa.entities().iter().filter(|e| e.arch == r.archetype("kid").unwrap()).collect();
    assert!(!kids.is_empty());
    for k in &kids {
        assert_eq!(sa.attr(k.id, "hp"), Some(20_000), "a summon joins its wave and gets the wave's scale");
        assert_eq!(sa.attr(k.id, "from"), Some(7), "spawner with is evaluated for the summoner");
        assert_eq!(k.wave, Some(1));
    }
}

/// Records what the generation was at each call.
struct GenProbe(Arc<Mutex<Vec<(u64, u64)>>>);

impl NativeHook for GenProbe {
    fn run(&self, cx: &mut NativeCtx<'_, '_>) -> Result<(), String> {
        self.0.lock().unwrap().push(cx.generation());
        if let Some(me) = cx.entity() {
            let mark = cx.rules().attr("mark").unwrap();
            cx.set(me, mark, cx.tick() as i64)?;
        }
        Ok(())
    }
}

#[test]
fn the_generation_moves_with_entities_not_with_attributes() {
    let items = with_base(
        r#""probe": {"kind": "native", "hook": "test.gen", "phase": "move", "reason": "a test"},
           "watcher": {"kind": "archetype", "attributes": {"mark": 0}, "behaviours": [{"type": "native", "hook": "probe"}]}"#,
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut n = Natives::new();
    n.insert("test.gen".into(), Arc::new(GenProbe(seen.clone())) as Arc<dyn NativeHook>);
    let mut s = Sim::new(rules(&items), 1, &n).unwrap();
    cmd(&mut s, "make", r#"{"what": "watcher", "at": "a"}"#).unwrap();
    s.step(&[]);
    s.step(&[]);
    cmd(&mut s, "make", r#"{"what": "dummy", "at": "b"}"#).unwrap();
    s.step(&[]);
    let mut c = s.clone();
    c.step(&[]);
    s.step(&[]);
    let g = seen.lock().unwrap().clone();
    assert_eq!(g[0], g[1], "attribute writes don't move it");
    assert_ne!(g[1], g[2], "a spawn does");
    assert_ne!(g[3].0, g[4].0, "a clone is another state");
}

/// Records the generation, and whether the entity has `warded`, at each call.
struct TagProbe(Arc<Mutex<Vec<(u64, bool)>>>);

impl NativeHook for TagProbe {
    fn run(&self, cx: &mut NativeCtx<'_, '_>) -> Result<(), String> {
        let warded = cx.entity().is_some_and(|me| cx.has_tag(me, "warded"));
        self.0.lock().unwrap().push((cx.generation().1, warded));
        Ok(())
    }
}

/// Review round 1 (finding 7): granted tags running out change what natives see, so the
/// generation moves on the tick they run out (a cache keyed on it can't serve stale tags).
#[test]
fn the_generation_moves_when_granted_tags_run_out() {
    let items = with_base(
        r#""probe": {"kind": "native", "hook": "test.tags", "phase": "move", "reason": "a test"},
           "ward": {"kind": "effect", "duration": "3t", "grant_tags": ["warded"]},
           "ward_it": {"kind": "command", "args": {"t": "entity"}, "do": [{"apply": "ward", "to": "arg.t"}]},
           "watcher": {"kind": "archetype", "attributes": {"mark": 0}, "behaviours": [{"type": "native", "hook": "probe"}]}"#,
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut n = Natives::new();
    n.insert("test.tags".into(), Arc::new(TagProbe(seen.clone())) as Arc<dyn NativeHook>);
    let mut s = Sim::new(rules(&items), 1, &n).unwrap();
    cmd(&mut s, "make", r#"{"what": "watcher", "at": "a"}"#).unwrap();
    s.step(&[]);
    let id = s.entities()[0].id;
    cmd(&mut s, "ward_it", &format!(r#"{{"t": {id}}}"#)).unwrap();
    for _ in 0..6 {
        s.step(&[]);
    }
    let g = seen.lock().unwrap().clone();
    // g[0] before the ward; g[1..] the six ticks after it.
    assert!(g[1].1, "warded: {g:?}");
    let out = g.iter().skip(1).position(|x| !x.1).expect("the ward runs out") + 1;
    assert_ne!(g[out - 1].0, g[out].0, "the generation moved on the tick the tags ran out: {g:?}");
    assert!(g[1..out].iter().all(|x| x.0 == g[1].0), "steady while warded: {g:?}");
    assert_eq!(g[g.len() - 2].0, g[g.len() - 1].0, "and settles once the effect is gone: {g:?}");
}

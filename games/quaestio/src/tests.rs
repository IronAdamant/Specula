//! Quest data checks and quests in the engine's real module host (linked in).

use super::*;
use specula::logic::tools::no_journal;
use specula::logic::LogicHost;
use specula::world::{EntityKey, Shape, SpawnSpec, World, WorldError};
use specula_core::Json;
use specula_logic::ModuleCode;

const HZ: u32 = 30;

#[test]
fn the_shipped_quests_parse_and_bad_data_names_its_place() {
    let d = Data::parse(DATA).unwrap();
    assert_eq!(d.quests["bones"].objectives.len(), 3);
    assert!(d.watched().contains(&"facultas_defeated".to_owned()));
    let base = |q: &str| format!(r#"{{"format": "specula.quaestio.v1", "quests": {{"q": {q}}}}}"#);
    let t = r#"{"key": "k", "text": "T"}"#;
    let cases = [
        (base(&format!(r#"{{"title": {t}, "summary": {t}, "objectives": []}}"#)), "quests.q.objectives"),
        (base(&format!(r#"{{"title": {t}, "summary": {t}, "objectives": [{{"id": "a", "text": {t}, "kind": "dance"}}]}}"#)), "objectives[0].kind"),
        (base(&format!(r#"{{"title": {t}, "summary": {t}, "objectives": [{{"id": "a", "text": {t}, "kind": "signal", "signal": "s", "after": ["b"]}}]}}"#)), "objectives[0].after"),
        (base(&format!(r#"{{"title": {t}, "summary": {t}, "objectives": [{{"id": "a", "text": {t}, "kind": "signal", "signal": "s"}}, {{"id": "a", "text": {t}, "kind": "signal", "signal": "s"}}]}}"#)), "objectives[1].id"),
        (base(&format!(r#"{{"title": {t}, "summary": {t}, "objectives": [{{"id": "a", "text": {t}, "kind": "have_item", "item": "x", "count": 0}}]}}"#)), "objectives[0].count"),
        (base(&format!(r#"{{"title": {t}, "summary": {t}, "objectives": [{{"id": "a", "text": {t}, "kind": "signal", "signal": "s"}}], "then": "nope"}}"#)), "starts a quest"),
        (base(&format!(r#"{{"title": {t}, "summary": {t}, "objectives": [{{"id": "a", "text": {t}, "kind": "signal", "signal": "s"}}], "rewards": [{{"money": 5}}]}}"#)), "rewards[0]"),
    ];
    for (text, place) in cases {
        let e = Data::parse(&text).unwrap_err();
        assert_eq!(e.code, "bad_data", "{text}");
        assert!(e.message.contains(place), "{} should name {place}", e.message);
    }
}

#[test]
fn records_latch_items_and_respect_order() {
    let d = Data::parse(DATA).unwrap();
    let q = &d.quests["bones"];
    let mut r = Record::default();
    let ret = &q.objectives[2];
    assert!(!r.open(q, ret), "return waits for collect");
    r.progress.insert("collect".into(), 3);
    assert!(r.open(q, ret));
    assert!(!r.complete(q));
    r.progress.insert("banish".into(), 2);
    r.progress.insert("return".into(), 1);
    assert!(r.complete(q));
    assert_eq!(Record::from_value(&r.to_value()).unwrap(), r);
    let m: BTreeMap<String, Value> = [("tag".to_owned(), Value::from("wisp"))].into();
    assert!(matches(&Value::map().with("tag", "wisp").with("caster", 3), &m));
    assert!(!matches(&Value::map().with("tag", "bat"), &m));
}

pub(crate) struct Run {
    pub host: LogicHost,
    pub world: World,
    pub tick: u64,
}

impl Run {
    pub fn new() -> Run {
        let mut r = Run { host: LogicHost::new(), world: World::default(), tick: 0 };
        r.host.load(ModuleCode::of::<Quaestio>(), Default::default(), Some(9), &mut r.world, 0, HZ, &mut no_journal()).expect("loads");
        let mut g = SpawnSpec::new(Shape::Group);
        g.name = Some("ambulo".into());
        r.world.spawn(g, "tester").unwrap();
        let mut p = SpawnSpec::new(Shape::Sphere);
        p.name = Some("player".into());
        p.parent = Some(EntityKey::Path("/ambulo".into()));
        p.props = Json::parse(r#"{"inventory": {"format": 1, "slots": [{"item": "bone", "count": 1}], "max_weight_g": 9000}}"#).unwrap();
        r.world.spawn(p, "tester").unwrap();
        r
    }

    fn player(&self) -> u64 {
        self.world.find_path("/ambulo/player").unwrap()
    }

    pub fn ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.host.tick(&mut self.world, self.tick, HZ, &mut no_journal());
            self.tick += 1;
        }
    }

    pub fn hook(&mut self, hook: &str, args: &str) -> Result<Json, WorldError> {
        self.host.call_hook(hook, &Json::parse(args).unwrap(), "test", "tester", &mut self.world, self.tick, HZ, &mut no_journal())
    }

    /// Emits an event as the module its name starts with would (`facultas_defeated` from a
    /// stand-in named facultas; anything else from "others").
    pub fn emit(&mut self, name: &str, data: Value) {
        let from = match name.split_once('_').map(|(m, _)| m) {
            Some("inventarium") => "inventarium",
            Some("facultas") => "facultas",
            _ => "others",
        };
        self.emit_as(from, name, data);
    }

    /// Emits an event from the stand-in module `from` (inventarium, facultas or others).
    pub fn emit_as(&mut self, from: &str, name: &str, data: Value) {
        if !self.host.running(from) {
            let code = match from {
                "inventarium" => ModuleCode::of::<Emitter<0>>(),
                "facultas" => ModuleCode::of::<Emitter<1>>(),
                _ => ModuleCode::of::<Emitter<2>>(),
            };
            self.host.load(code, Default::default(), None, &mut self.world, self.tick, HZ, &mut no_journal()).unwrap();
        }
        let args = Json::obj([("name", Json::from(name)), ("data", data.to_json())]);
        self.host.call_hook(&format!("{from}/emit"), &args, "test", "tester", &mut self.world, self.tick, HZ, &mut no_journal()).unwrap();
        self.ticks(1);
    }

    pub fn quest(&self) -> Json {
        self.world.get(self.player()).unwrap().props.get("quests").and_then(|q| q.get("bones")).cloned().unwrap_or(Json::Null)
    }

    pub fn events(&self, name: &str) -> Vec<Json> {
        let s = self.host.status(None, true).unwrap();
        s.get("events").and_then(Json::as_array).unwrap_or_default().iter().filter(|e| e.get("name").and_then(Json::as_str) == Some(name)).map(|e| e.get("data").cloned().unwrap_or(Json::Null)).collect()
    }
}

/// Emits any event its `emit` hook names, standing in for inventarium (0), facultas (1) or any
/// other module (2).
#[derive(Default)]
struct Emitter<const N: usize>;
impl<const N: usize> Module for Emitter<N> {
    fn describe(&self) -> ModuleInfo {
        let name = ["inventarium", "facultas", "others"][N];
        ModuleInfo { name, hooks: vec![specula_sdk::HookDef { name: "emit", doc: "emits an event", args: r#"{"type": "object"}"#}], ..ModuleInfo::default() }
    }
    fn hook(&self, cx: &mut Cx, _name: &str, args: &Value) -> Result<Value, SdkError> {
        let name = args.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
        cx.emit(&name, args.get("data").cloned().unwrap_or_default())?;
        Ok(Value::Null)
    }
}

#[test]
fn a_quest_runs_from_start_to_rewards_on_events() {
    let mut r = Run::new();
    let p = r.player();
    r.emit("quaestio_start", Value::map().with("actor", p).with("quest", "bones"));
    r.ticks(1);
    let q = r.quest();
    assert_eq!(q.get("state").and_then(Json::as_str), Some("active"));
    assert_eq!(q.get("progress").and_then(|p| p.get("collect")).and_then(Json::as_u64), Some(1), "the bone already carried counts");
    // Two more bones, then they're spent: collect stays met (latched).
    r.emit("inventarium_changed", Value::map().with("owner", p).with("item", "bone").with("delta", 2).with("count", 3));
    r.ticks(1);
    r.emit("inventarium_changed", Value::map().with("owner", p).with("item", "bone").with("delta", -3).with("count", 0));
    r.ticks(1);
    assert_eq!(r.quest().get("progress").and_then(|p| p.get("collect")).and_then(Json::as_u64), Some(3));
    // Wisps: one that isn't a wisp, one credited to someone else, then two of ours.
    r.emit("facultas_defeated", Value::map().with("caster", p).with("tag", "bat"));
    r.emit("facultas_defeated", Value::map().with("caster", 99999).with("tag", "wisp"));
    r.emit("facultas_defeated", Value::map().with("caster", p).with("tag", "wisp"));
    r.emit("facultas_defeated", Value::map().with("caster", p).with("tag", "wisp"));
    r.emit("facultas_defeated", Value::map().with("caster", p).with("tag", "wisp"));
    r.ticks(1);
    assert_eq!(r.quest().get("progress").and_then(|p| p.get("banish")).and_then(Json::as_u64), Some(2), "capped at the target");
    r.hook("quaestio/signal", &format!(r#"{{"actor": {p}, "signal": "bones_returned"}}"#)).unwrap();
    assert_eq!(r.quest().get("state").and_then(Json::as_str), Some("done"));
    r.ticks(1);
    // Rewards went out as requests to the inventory and ability modules.
    let gives = r.events("inventarium_give");
    assert_eq!(gives.last().and_then(|g| g.get("item")).and_then(Json::as_str), Some("silver_key"));
    assert_eq!(r.events("facultas_learn").last().and_then(|g| g.get("ability")).and_then(Json::as_str), Some("ward"));
    assert_eq!(r.events("quaestio_completed").len(), 1);
    // Tracker text in the world, with the keys of what it shows.
    let t = r.world.get(r.world.find_path("/quaestio/tracker").unwrap()).unwrap();
    assert_eq!(t.props.get("keys").and_then(Json::as_array).unwrap()[0].as_str(), Some("quest.bones.title"));
    let e = r.hook("quaestio/start", &format!(r#"{{"actor": {p}, "quest": "bones"}}"#)).unwrap_err();
    assert!(format!("{e:?}").contains("already"), "{e:?}");
    let sounds: Vec<String> = r.host.take_sounds().into_iter().map(|s| s.name).collect();
    assert_eq!(sounds, vec!["quest_start", "quest_done"]);
}

/// Review of m5-gmods (minor): any module may emit any event, so notices count only from the
/// module they name; a forged `facultas_defeated` or `inventarium_changed` credits nothing.
#[test]
fn forged_notices_credit_nothing() {
    let mut r = Run::new();
    let p = r.player();
    r.emit("quaestio_start", Value::map().with("actor", p).with("quest", "bones"));
    r.ticks(1);
    let progress = |r: &Run, k: &str| r.quest().get("progress").and_then(|p| p.get(k)).and_then(Json::as_u64);
    for _ in 0..3 {
        r.emit_as("others", "facultas_defeated", Value::map().with("caster", p).with("tag", "wisp"));
    }
    r.emit_as("others", "inventarium_changed", Value::map().with("owner", p).with("item", "bone").with("delta", 2).with("count", 3));
    r.ticks(1);
    assert_eq!((progress(&r, "banish").unwrap_or(0), progress(&r, "collect")), (0, Some(1)), "{}", r.quest());
    assert!(r.events("quaestio_refused").iter().any(|e| e.get("code").and_then(Json::as_str) == Some("forged")));
    // The real ones count.
    r.emit("facultas_defeated", Value::map().with("caster", p).with("tag", "wisp"));
    r.ticks(1);
    assert_eq!(progress(&r, "banish"), Some(1));
}

#[test]
fn hooks_start_abandon_and_report() {
    let mut r = Run::new();
    // Another quest's record (from other data) must survive this one's writes: props merge only
    // one level deep, so the whole map is written back.
    let other = Json::parse(r#"{"format": 1, "state": "done", "progress": {}, "started": 1, "finished": 2}"#).unwrap();
    let mut quests = Json::parse("{}").unwrap();
    quests.set("old_quest", other.clone());
    let mut props = Json::parse("{}").unwrap();
    props.set("quests", quests);
    let up = specula::world::UpdateSpec { props: Some(props), ..Default::default() };
    r.world.update(&EntityKey::Path("/ambulo/player".into()), up, "tester").unwrap();
    let old = |r: &Run| r.world.get(r.player()).unwrap().props.get("quests").and_then(|q| q.get("old_quest")).map(|o| (o.get("state").cloned(), o.get("finished").cloned()));
    let other = Some((Some(Json::from("done")), Some(Json::from(2u64))));
    let st = r.hook("quaestio/start", r#"{"actor": "/ambulo/player", "quest": "bones"}"#).unwrap();
    let objectives = st.get("quests").and_then(|q| q.get("bones")).and_then(|b| b.get("objectives")).and_then(Json::as_array).unwrap().to_vec();
    assert_eq!(objectives.len(), 3);
    assert_eq!(objectives[2].get("open"), Some(&Json::Bool(false)), "return waits for collect");
    assert_eq!(old(&r), other.clone(), "starting kept the other quest");
    r.hook("quaestio/abandon", r#"{"actor": "/ambulo/player", "quest": "bones"}"#).unwrap();
    assert_eq!(r.quest(), Json::Null);
    assert_eq!(old(&r), other, "abandoning kept the other quest");
    let e = r.hook("quaestio/start", r#"{"actor": "/ambulo/player", "quest": "dragons"}"#).unwrap_err();
    assert!(format!("{e:?}").contains("unknown_quest"));
}

#[test]
fn twin_runs_match_and_a_saved_quest_continues_exactly() {
    let script = |r: &mut Run| {
        let p = r.player();
        r.emit("quaestio_start", Value::map().with("actor", p).with("quest", "bones"));
        r.emit("facultas_defeated", Value::map().with("caster", p).with("tag", "wisp"));
        r.ticks(2);
    };
    let (mut a, mut b) = (Run::new(), Run::new());
    script(&mut a);
    script(&mut b);
    assert_eq!(a.world.state_hash(), b.world.state_hash());
    assert_eq!(a.host.state_hash(), b.host.state_hash());
    let bytes = a.host.save_state().unwrap();
    let files = specula::persist::render(&a.world);
    let world = specula::persist::load_with(|f| files.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| specula::persist::PersistError::new("not_found", f.to_owned()))).unwrap().0;
    let mut c = Run { host: LogicHost::new(), world, tick: a.tick };
    c.host.load(ModuleCode::of::<Quaestio>(), Default::default(), Some(9), &mut c.world, c.tick, HZ, &mut no_journal()).unwrap();
    // The stand-ins the saving run loaded, in the same order.
    for (name, code) in [("inventarium", ModuleCode::of::<Emitter<0>>()), ("facultas", ModuleCode::of::<Emitter<1>>()), ("others", ModuleCode::of::<Emitter<2>>())] {
        if a.host.running(name) {
            c.host.load(code, Default::default(), None, &mut c.world, c.tick, HZ, &mut no_journal()).unwrap();
        }
    }
    assert!(c.host.restore_state(&bytes).unwrap().is_empty());
    let p = a.player();
    a.emit("facultas_defeated", Value::map().with("caster", p).with("tag", "wisp"));
    c.emit("facultas_defeated", Value::map().with("caster", p).with("tag", "wisp"));
    a.ticks(1);
    c.ticks(1);
    assert_eq!(a.quest().get("progress").and_then(|p| p.get("banish")).and_then(Json::as_u64), Some(2));
    assert_eq!(a.world.state_hash(), c.world.state_hash());
    assert_eq!(a.host.state_hash(), c.host.state_hash());
}

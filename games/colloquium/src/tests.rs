//! Dialogue data checks, conditions, and talks in the engine's real module host (linked in).

use super::*;
use specula::logic::tools::no_journal;
use specula::logic::LogicHost;
use specula::world::{EntityKey, Shape, SpawnSpec, World, WorldError};
use specula_core::Json;
use specula_logic::ModuleCode;

const HZ: u32 = 30;

#[test]
fn the_shipped_dialogue_parses_and_references_are_checked() {
    let d = Data::parse(DATA).unwrap();
    assert_eq!(d.graphs["sexton"].start, "greet");
    let base = |nodes: &str| format!(r#"{{"format": "specula.colloquium.v1", "speakers": {{"a": {{"name": {{"key": "a", "text": "A"}}}}}}, "graphs": {{"g": {{"start": "n", "nodes": {nodes}}}}}}}"#);
    let line = r#"{"key": "k", "text": "T"}"#;
    let cases = [
        (base(&format!(r#"{{"n": {{"speaker": "a", "line": {line}, "next": "gone"}}}}"#)), "graphs.g.nodes.n.next"),
        (base(&format!(r#"{{"n": {{"speaker": "b", "line": {line}}}}}"#)), "graphs.g.nodes.n.speaker"),
        (base(&format!(r#"{{"n": {{"speaker": "a", "line": {line}, "choices": [{{"line": {line}, "next": "x"}}]}}}}"#)), "graphs.g.nodes.n.choices[0].next"),
        (base(&format!(r#"{{"n": {{"speaker": "a", "line": {line}, "choices": [{{"line": {line}}}], "next": "n"}}}}"#)), "has both choices and next"),
        (base(&format!(r#"{{"n": {{"speaker": "a", "line": {line}, "choices": [{{"line": {line}, "if": {{"weather": "rain"}}}}]}}}}"#)), "choices[0].if.weather"),
        (base(&format!(r#"{{"n": {{"speaker": "a", "line": {line}, "do": [{{"explode": 1}}]}}}}"#)), "do[0].explode"),
        (base(r#"{"n": {"speaker": "a", "line": {"key": "Bad Key", "text": "x"}}}"#), "line.key"),
        (base(&format!(r#"{{"m": {{"speaker": "a", "line": {line}}}}}"#)), "graphs.g.start"),
    ];
    for (text, place) in cases {
        let e = Data::parse(&text).unwrap_err();
        assert_eq!(e.code, "bad_data", "{text}");
        assert!(e.message.contains(place), "{} should name {place}", e.message);
    }
    // Conditions are capped in depth and size before they are evaluated.
    let mut deep = r#"{"flag": "f"}"#.to_owned();
    for _ in 0..=MAX_CONDITION_DEPTH {
        deep = format!(r#"{{"not": {deep}}}"#);
    }
    let text = base(&format!(r#"{{"n": {{"speaker": "a", "line": {line}, "choices": [{{"line": {line}, "if": {deep}}}]}}}}"#));
    assert!(Data::parse(&text).unwrap_err().message.contains("nests deeper"));
    let wide = format!(r#"{{"any": [{}]}}"#, vec![r#"{"flag": "f"}"#; MAX_CONDITION_NODES].join(","));
    let text = base(&format!(r#"{{"n": {{"speaker": "a", "line": {line}, "choices": [{{"line": {line}, "if": {wide}}}]}}}}"#));
    assert!(Data::parse(&text).unwrap_err().message.contains("parts"));
}

#[test]
fn conditions_read_inventory_quests_and_flags() {
    let actor = specula_sdk::Entity {
        id: Id(1),
        data: Value::parse_json(r#"{"props": {"inventory": {"slots": [{"item": "bone", "count": 2}, null, {"item": "bone", "count": 1}]}, "quests": {"bones": {"state": "active"}}, "colloquium": {"flags": {"met": true}}}}"#).unwrap(),
    };
    let f = Facts { actor: &actor };
    let c = |s: &str| {
        let mut budget = MAX_CONDITION_NODES;
        cond(&Value::parse_json(s).unwrap(), "t", 0, &mut budget).unwrap()
    };
    assert!(f.holds(&c(r#"{"has_item": ["bone", 3]}"#)));
    assert!(!f.holds(&c(r#"{"has_item": ["bone", 4]}"#)));
    assert!(f.holds(&c(r#"{"quest": ["bones", "active"]}"#)));
    assert!(f.holds(&c(r#"{"quest": ["other", "none"]}"#)));
    assert!(f.holds(&c(r#"{"all": [{"flag": "met"}, {"not": {"flag": "angry"}}]}"#)));
    assert!(!f.holds(&c(r#"{"any": [{"flag": "angry"}, {"quest": ["bones", "done"]}]}"#)));
}

pub(crate) struct Run {
    pub host: LogicHost,
    pub world: World,
    pub tick: u64,
}

impl Run {
    pub fn new() -> Run {
        let mut r = Run { host: LogicHost::new(), world: World::default(), tick: 0 };
        r.host.load(ModuleCode::of::<Colloquium>(), Default::default(), Some(5), &mut r.world, 0, HZ, &mut no_journal()).expect("loads");
        let mut g = SpawnSpec::new(Shape::Group);
        g.name = Some("ambulo".into());
        r.world.spawn(g, "tester").unwrap();
        let mut p = SpawnSpec::new(Shape::Sphere);
        p.name = Some("player".into());
        p.parent = Some(EntityKey::Path("/ambulo".into()));
        r.world.spawn(p, "tester").unwrap();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("sexton".into());
        s.position = specula_core::math::Vec3::new(1.5, 0.0, 0.0);
        s.tags = ["talkable".to_owned()].into();
        s.props = Json::parse(r#"{"dialogue": "sexton"}"#).unwrap();
        r.world.spawn(s, "tester").unwrap();
        r
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

    pub fn press(&mut self, action: &str) {
        self.host.hold("colloquium", &Json::parse(&format!(r#"{{"{action}": true}}"#)).unwrap(), 1).unwrap();
        self.ticks(1);
        self.host.hold("colloquium", &Json::parse(&format!(r#"{{"{action}": false}}"#)).unwrap(), 1).unwrap();
        self.ticks(1);
    }

    pub fn talk(&self) -> Json {
        let id = self.world.find_path("/ambulo/player").unwrap();
        self.world.get(id).unwrap().props.get("colloquium").cloned().unwrap_or(Json::Null)
    }

    pub fn events(&self, name: &str) -> Vec<Json> {
        let s = self.host.status(None, true).unwrap();
        s.get("events").and_then(Json::as_array).unwrap_or_default().iter().filter(|e| e.get("name").and_then(Json::as_str) == Some(name)).map(|e| e.get("data").cloned().unwrap_or(Json::Null)).collect()
    }

    fn texts(&self) -> Vec<(String, String)> {
        self.world
            .entities()
            .filter(|e| e.tags.contains("colloquium_text"))
            .map(|e| (e.name.clone(), e.props.get("line").and_then(|l| l.get("key")).and_then(Json::as_str).unwrap_or_default().to_owned()))
            .collect()
    }
}

#[test]
fn a_talk_by_input_offers_the_choices_whose_conditions_hold_and_asks_other_modules() {
    let mut r = Run::new();
    r.press("talk");
    assert_eq!(r.talk().get("node").and_then(Json::as_str), Some("greet"));
    // Quest "bones" is none: "Can I help?" and "Farewell." (choices 0 and 3).
    assert_eq!(r.talk().get("choices"), Some(&Json::parse("[0, 3]").unwrap()));
    let texts = r.texts();
    assert_eq!(texts.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["line", "choice_1", "choice_2"]);
    assert_eq!(texts[0].1, "dialogue.sexton.greet", "the line's key travels with its text");
    r.press("choice_1");
    assert_eq!(r.talk().get("node").and_then(Json::as_str), Some("offer"));
    r.press("choice_1");
    assert_eq!(r.talk().get("node").and_then(Json::as_str), Some("accepted"));
    // The choice asked the quest module to start "bones"; entering "accepted" asked the
    // inventory module for the lantern.
    assert_eq!(r.events("quaestio_start")[0].get("quest").and_then(Json::as_str), Some("bones"));
    assert_eq!(r.events("inventarium_give")[0].get("item").and_then(Json::as_str), Some("lantern"));
    // Talk at a node without choices goes on; at its end, the talk ends and the texts go.
    r.press("talk");
    assert_eq!(r.talk().get("node"), Some(&Json::Null));
    assert!(r.texts().is_empty());
    r.ticks(1);
    assert_eq!(r.events("colloquium_ended").len(), 1);
    // A sound per line entered: greet, offer, accepted.
    assert_eq!(r.host.take_sounds().iter().filter(|s| s.name == "talk").count(), 3);
}

#[test]
fn hooks_drive_a_talk_and_walking_away_ends_it() {
    let mut r = Run::new();
    let st = r.hook("colloquium/start", r#"{"actor": "/ambulo/player", "graph": "sexton", "with": "/sexton"}"#).unwrap();
    assert_eq!(st.get("waits_for").and_then(Json::as_str), Some("choice"));
    assert_eq!(st.get("choices").and_then(Json::as_array).map(|c| c.len()), Some(2));
    let e = r.hook("colloquium/choose", r#"{"actor": "/ambulo/player", "choice": 3}"#).unwrap_err();
    assert!(format!("{e:?}").contains("no_such_choice"), "{e:?}");
    let e = r.hook("colloquium/next", r#"{"actor": "/ambulo/player"}"#).unwrap_err();
    assert!(format!("{e:?}").contains("choose_first"));
    let e = r.hook("colloquium/start", r#"{"actor": "/ambulo/player", "graph": "nobody"}"#).unwrap_err();
    assert!(format!("{e:?}").contains("unknown_graph"));
    // The player walks 10 m off: the talk ends at the next tick.
    let away = specula::world::UpdateSpec { position: Some(specula_core::math::Vec3::new(10.0, 0.0, 0.0)), ..Default::default() };
    r.world.update(&EntityKey::Path("/ambulo/player".into()), away, "tester").unwrap();
    r.ticks(1);
    assert_eq!(r.talk().get("node"), Some(&Json::Null));
    r.ticks(1);
    assert_eq!(r.events("colloquium_ended")[0].get("how").and_then(Json::as_str), Some("walked_away"));
}

#[test]
fn twin_runs_match_and_a_saved_talk_continues_exactly() {
    let script = |r: &mut Run| {
        r.press("talk");
        r.press("choice_1");
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
    c.host.load(ModuleCode::of::<Colloquium>(), Default::default(), Some(5), &mut c.world, c.tick, HZ, &mut no_journal()).unwrap();
    assert!(c.host.restore_state(&bytes).unwrap().is_empty());
    assert_eq!(c.talk().get("node").and_then(Json::as_str), Some("offer"), "the talk is in the saved world");
    for action in ["choice_1", "talk"] {
        a.press(action);
        c.press(action);
        assert_eq!(a.world.state_hash(), c.world.state_hash());
        assert_eq!(a.host.state_hash(), c.host.state_hash());
    }
}

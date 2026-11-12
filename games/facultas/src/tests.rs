//! Cooldowns, costs and regeneration as integer arithmetic, then the module in the engine's real
//! module host (linked in).

use super::*;
use specula::logic::tools::no_journal;
use specula::logic::LogicHost;
use specula::world::{EntityKey, Shape, SpawnSpec, World, WorldError};
use specula_core::Json;
use specula_logic::ModuleCode;

const HZ: u32 = 30;

fn data() -> Data {
    Data::parse(DATA).expect("the shipped abilities.json parses")
}

#[test]
fn ticks_round_up_and_regeneration_is_exact_integers() {
    assert_eq!(ticks_for(2500, 30), 75);
    assert_eq!(ticks_for(1, 30), 1, "any cooldown lasts at least a tick");
    assert_eq!(ticks_for(0, 30), 0);
    let r = Resource { max: 100, regen_per_s: 12 };
    let l = Level { milli: 0, at: 10 };
    assert_eq!(l.now(&r, 10, 30), 0);
    assert_eq!(l.now(&r, 11, 30), 400, "12/s at 30 Hz: 0.4 a tick");
    assert_eq!(l.now(&r, 40, 30), 12_000);
    assert_eq!(l.now(&r, 10_000, 30), 100_000, "capped at the maximum");
    assert_eq!(l.now(&r, 5, 30), 0, "a level from the future doesn't go negative");
    assert_eq!(Level { milli: 1, at: 0 }.now(&Resource { max: MAX_RESOURCE, regen_per_s: MAX_RESOURCE }, u64::MAX, 1), MAX_RESOURCE * 1000, "saturates, never overflows");
}

#[test]
fn checks_name_why_a_cast_is_refused_and_paying_starts_the_cooldown() {
    let d = data();
    let mut c = Caster::new(&d, 0);
    let lantern = |item: &str| u64::from(item == "lantern");
    let none = |_: &str| 0;
    assert_eq!(c.check(&d, "flare", 0, HZ, &none).unwrap_err().code, "missing_item");
    c.check(&d, "flare", 0, HZ, &lantern).unwrap();
    assert_eq!(c.check(&d, "ward", 0, HZ, &lantern).unwrap_err().code, "not_known");
    assert_eq!(c.check(&d, "nope", 0, HZ, &lantern).unwrap_err().code, "unknown_ability");
    c.pay(&d, "flare", 0, HZ);
    let e = c.check(&d, "flare", 10, HZ, &lantern).unwrap_err();
    assert_eq!((e.code.as_str(), e.details.get("ticks_left").and_then(Value::as_u64)), ("cooldown", Some(65)));
    c.check(&d, "flare", 75, HZ, &lantern).unwrap();
    // Stamina 100 - 35 = 65 at tick 0, +0.4 a tick: two more flares at 75 and 150 leave
    // 65 + 30 - 35 = 60, then 60 + 30 - 35 = 55.
    c.pay(&d, "flare", 75, HZ);
    c.pay(&d, "flare", 150, HZ);
    assert_eq!(c.levels["stamina"], Level { milli: 55_000, at: 150 });
    c.known.push("ward".into());
    let e = Caster { levels: [("stamina".to_owned(), Level { milli: 59_999, at: 0 })].into(), ..c.clone() }.check(&d, "ward", 0, HZ, &lantern).unwrap_err();
    assert_eq!(e.code, "no_resource");
    // Round trip through props, and a caster saved before a resource existed gets it full.
    let v = c.to_value();
    assert_eq!(Caster::from_value(&v, &d, 999).unwrap(), c);
    let old = Value::parse_json(r#"{"format": 1, "known": ["flare"], "ready_at": {}, "resources": {}}"#).unwrap();
    assert_eq!(Caster::from_value(&old, &d, 7).unwrap().levels["stamina"], Level { milli: 100_000, at: 7 });
    for bad in [r#"{"format": 2}"#, r#"{"format": 1, "known": [1]}"#, r#"{"format": 1, "known": [], "ready_at": {"flare": "x"}}"#] {
        assert_eq!(Caster::from_value(&Value::parse_json(bad).unwrap(), &d, 0).unwrap_err().code, "bad_caster", "{bad}");
    }
}

#[test]
fn bad_data_names_its_place() {
    let base = |abilities: &str| format!(r#"{{"format": "specula.facultas.v1", "resources": {{"mana": {{"name": {{"key": "m", "text": "M"}}, "max": 10, "regen_per_s": 1}}}}, "abilities": {abilities}}}"#);
    let ok = r#"{"name": {"key": "a", "text": "A"}, "summary": {"key": "s", "text": "S"}, "cooldown_ms": 1, "effects": [{"kind": "emit", "event": "x"}]"#;
    let cases = [
        (base("{}"), "abilities"),
        (base(&format!(r#"{{"a": {ok}, "cost": {{"gold": 1}}}}}}"#)), "abilities.a.cost.gold"),
        (base(&format!(r#"{{"a": {ok}, "cost": {{"mana": 11}}}}}}"#)), "abilities.a.cost.mana"),
        (base(r#"{"a": {"name": {"key": "a", "text": "A"}, "summary": {"key": "s", "text": "S"}, "cooldown_ms": 1, "effects": [{"kind": "boom"}]}}"#), "abilities.a.effects[0].kind"),
        (base(r#"{"a": {"name": {"key": "a", "text": "A"}, "summary": {"key": "s", "text": "S"}, "cooldown_ms": 700000, "effects": [{"kind": "emit", "event": "x"}]}}"#), "abilities.a.cooldown_ms"),
        (base(&format!(r#"{{"a": {ok}, "action": "ability_1"}}, "b": {ok}, "action": "ability_1"}}}}"#)), "abilities.b.action"),
        (base(r#"{"a": {"name": {"key": "a", "text": "A"}, "summary": {"key": "s", "text": "S"}, "cooldown_ms": 1, "effects": [{"kind": "vfx", "effect": "impact", "tint": "cyan", "ms": 5}]}}"#), "abilities.a.effects[0].tint"),
    ];
    for (text, place) in cases {
        let e = Data::parse(&text).unwrap_err();
        assert_eq!(e.code, "bad_data", "{text}");
        assert!(e.message.contains(place), "{} should name {place}", e.message);
    }
}

pub(crate) struct Run {
    pub host: LogicHost,
    pub world: World,
    pub tick: u64,
}

impl Run {
    pub fn new() -> Run {
        let mut r = Run { host: LogicHost::new(), world: World::default(), tick: 0 };
        r.host.load(ModuleCode::of::<Facultas>(), Default::default(), Some(3), &mut r.world, 0, HZ, &mut no_journal()).expect("loads");
        let mut g = SpawnSpec::new(Shape::Group);
        g.name = Some("ambulo".into());
        r.world.spawn(g, "tester").unwrap();
        let mut p = SpawnSpec::new(Shape::Sphere);
        p.name = Some("player".into());
        p.parent = Some(EntityKey::Path("/ambulo".into()));
        p.props = Json::parse(r#"{"inventory": {"format": 1, "slots": [{"item": "lantern", "count": 1}], "max_weight_g": 9000}}"#).unwrap();
        r.world.spawn(p, "tester").unwrap();
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
        self.host.hold("facultas", &Json::parse(&format!(r#"{{"{action}": true}}"#)).unwrap(), 1, None).unwrap();
        self.ticks(1);
        self.host.hold("facultas", &Json::parse(&format!(r#"{{"{action}": false}}"#)).unwrap(), 1, None).unwrap();
        self.ticks(1);
    }

    pub fn wisp(&mut self, name: &str, x: f32, hp: i64) {
        let mut s = SpawnSpec::new(Shape::Sphere);
        s.name = Some(name.into());
        s.position = specula_core::math::Vec3::new(x, 0.0, 0.0);
        s.tags = ["wisp".to_owned()].into();
        s.props = Json::parse(&format!(r#"{{"hp": {hp}}}"#)).unwrap();
        self.world.spawn(s, "tester").unwrap();
    }

    pub fn entity(&self, path: &str) -> &specula::world::Entity {
        self.world.get(self.world.find_path(path).unwrap()).unwrap()
    }

    pub fn events(&self, name: &str) -> Vec<Json> {
        let s = self.host.status(None, true).unwrap();
        s.get("events").and_then(Json::as_array).unwrap_or_default().iter().filter(|e| e.get("name").and_then(Json::as_str) == Some(name)).map(|e| e.get("data").cloned().unwrap_or(Json::Null)).collect()
    }
}

#[test]
fn a_pressed_action_casts_with_effects_and_a_second_press_hits_the_cooldown() {
    let mut r = Run::new();
    r.wisp("near", 3.0, 100);
    r.wisp("far", 9.0, 100);
    r.press("ability_1");
    assert_eq!(r.entity("/near").props.get("hp").and_then(Json::as_f64), Some(40.0), "60 damage in 5 m");
    assert_eq!(r.entity("/far").props.get("hp").and_then(Json::as_f64), Some(100.0), "out of reach");
    // Effects: two particle entities under /facultas, a sound queued, a cast event.
    let fx: Vec<_> = r.world.entities().filter(|e| e.tags.contains("facultas_fx")).map(|e| e.vfx.as_ref().map(|v| v.effect.clone())).collect();
    assert_eq!(fx, vec![Some("impact".to_owned()), Some("sparks".to_owned())]);
    let sounds = r.host.take_sounds();
    assert_eq!(sounds.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["flare"]);
    assert!(sounds[0].at.is_some(), "placed at the caster");
    r.press("ability_1");
    let refused = r.events("facultas_refused");
    assert_eq!(refused.last().and_then(|e| e.get("code")).and_then(Json::as_str), Some("cooldown"), "{refused:?}");
    // The effects expire on their own (900 ms = 27 ticks).
    r.ticks(30);
    assert_eq!(r.world.entities().filter(|e| e.tags.contains("facultas_fx")).count(), 0);
    // Second flare after the cooldown defeats the wisp: hidden, flagged, an event.
    r.ticks(50);
    r.press("ability_1");
    let near = r.entity("/near");
    assert_eq!((near.props.get("defeated"), near.visible), (Some(&Json::Bool(true)), false));
    r.ticks(1);
    assert_eq!(r.events("facultas_defeated").len(), 1);
}

#[test]
fn hooks_learn_cast_and_report_status_and_failures_change_nothing() {
    let mut r = Run::new();
    let e = r.hook("facultas/cast", r#"{"caster": "/ambulo/player", "ability": "ward"}"#).unwrap_err();
    assert!(format!("{e:?}").contains("not_known"), "{e:?}");
    let before = r.entity("/ambulo/player").props.clone();
    r.hook("facultas/learn", r#"{"caster": "/ambulo/player", "ability": "ward"}"#).unwrap();
    // A status from elsewhere stays when the ward adds its own (props merge one level deep).
    let blessed = specula::world::UpdateSpec { props: Some(Json::parse(r#"{"status": {"blessed": 9999}}"#).unwrap()), ..Default::default() };
    r.world.update(&EntityKey::Path("/ambulo/player".into()), blessed, "tester").unwrap();
    let out = r.hook("facultas/cast", r#"{"caster": "/ambulo/player", "ability": "ward"}"#).unwrap();
    assert_eq!(out.get("ready_at").and_then(Json::as_u64), Some(240), "8 s at 30 Hz");
    let status = r.entity("/ambulo/player").props.get("status").cloned().unwrap();
    assert_eq!(status, Json::parse(r#"{"blessed": 9999, "warded": 120}"#).unwrap());
    let st = r.hook("facultas/status", r#"{"caster": "/ambulo/player"}"#).unwrap();
    assert_eq!(st.get("ticks_until_ready").and_then(|t| t.get("ward")).and_then(Json::as_u64), Some(240));
    assert_eq!(st.get("resources").and_then(|t| t.get("stamina")).and_then(|s| s.get("now")).and_then(Json::as_f64), Some(40.0));
    // Not enough stamina for a flare and a dash right after... a flare (35) fits in 40, a ward doesn't.
    let e = r.hook("facultas/cast", r#"{"caster": "/ambulo/player", "ability": "dash"}"#).unwrap_err();
    assert!(format!("{e:?}").contains("not_known"));
    r.hook("facultas/cast", r#"{"caster": "/ambulo/player", "ability": "flare"}"#).unwrap();
    let after = r.entity("/ambulo/player").props.clone();
    let e = r.hook("facultas/cast", r#"{"caster": "/ambulo/player", "ability": "flare"}"#).unwrap_err();
    assert!(format!("{e:?}").contains("cooldown"), "{e:?}");
    assert_eq!(r.entity("/ambulo/player").props, after, "a refused cast changes nothing");
    assert_ne!(before, after);
    let e = r.hook("facultas/cast", r#"{"caster": 999999, "ability": "flare"}"#).unwrap_err();
    assert!(format!("{e:?}").contains("not_found"));
}

/// A quest reward arrives as an event from another module.
#[derive(Default)]
struct Rewarder;
impl Module for Rewarder {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "rewarder", ..ModuleInfo::default() }
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        if cx.tick() == 0 {
            cx.emit("facultas_learn", Value::map().with("caster", "/ambulo/player").with("ability", "dash"))?;
            cx.emit("facultas_learn", Value::map().with("caster", "/ambulo/player").with("ability", "nope"))?;
        }
        Ok(())
    }
}

#[test]
fn events_teach_abilities_and_refusals_come_back() {
    let mut r = Run::new();
    r.host.load(ModuleCode::of::<Rewarder>(), Default::default(), None, &mut r.world, 0, HZ, &mut no_journal()).unwrap();
    r.ticks(3);
    let known = r.entity("/ambulo/player").props.get("abilities").and_then(|a| a.get("known")).cloned().unwrap();
    assert_eq!(known, Json::parse(r#"["flare", "dash"]"#).unwrap());
    let refused = r.events("facultas_refused");
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].get("code").and_then(Json::as_str), Some("unknown_ability"));
    r.press("ability_3");
    r.ticks(1);
    assert_eq!(r.events("facultas_dashed").len(), 1, "the emit effect");
}

#[test]
fn twin_runs_match_and_a_saved_run_continues_exactly() {
    let script = |r: &mut Run| {
        r.wisp("w", 2.0, 130);
        r.press("ability_1");
        r.ticks(80);
        r.press("ability_1");
        r.ticks(10);
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
    c.host.load(ModuleCode::of::<Facultas>(), Default::default(), Some(3), &mut c.world, c.tick, HZ, &mut no_journal()).unwrap();
    assert!(c.host.restore_state(&bytes).unwrap().is_empty());
    for _ in 0..3 {
        a.ticks(30);
        c.ticks(30);
        a.press("ability_1");
        c.press("ability_1");
        assert_eq!(a.world.state_hash(), c.world.state_hash());
        assert_eq!(a.host.state_hash(), c.host.state_hash());
    }
}

/// Emits `facultas_request {caster: /cN, ability: flare}` for N in 0..COUNT at tick 0, as other
/// modules (or a quest) would.
#[derive(Default)]
struct Requests<const COUNT: usize>;
impl<const COUNT: usize> Module for Requests<COUNT> {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "requests", ..ModuleInfo::default() }
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        if cx.tick() == 0 {
            for i in 0..COUNT {
                cx.emit("facultas_request", Value::map().with("caster", format!("/c{i}").as_str()).with("ability", "flare"))?;
            }
        }
        Ok(())
    }
}

/// Casters `/c0..` 1 km apart, each with a lantern and a wisp 2 m away (`/w0..`).
fn casters(r: &mut Run, n: usize) {
    for i in 0..n {
        let mut p = SpawnSpec::new(Shape::Sphere);
        p.name = Some(format!("c{i}"));
        p.position = specula_core::math::Vec3::new(1000.0 * i as f32, 0.0, 0.0);
        p.props = Json::parse(r#"{"inventory": {"format": 1, "slots": [{"item": "lantern", "count": 1}], "max_weight_g": 9000}}"#).unwrap();
        r.world.spawn(p, "tester").unwrap();
        r.wisp(&format!("w{i}"), 1000.0 * i as f32 + 2.0, 100);
    }
}

/// Review of m5-gmods (major 2): two casters flaring in one tick both hit (their effect
/// entities used to collide on a name, and the second cast was refused after paying).
#[test]
fn two_casters_flare_in_one_tick_and_both_hit() {
    let mut r = Run::new();
    casters(&mut r, 2);
    r.host.load(ModuleCode::of::<Requests<2>>(), Default::default(), None, &mut r.world, 0, HZ, &mut no_journal()).unwrap();
    // Tick 0 asks, tick 1 casts, tick 2 delivers what the casts emitted.
    r.ticks(3);
    assert_eq!(r.events("facultas_cast").len(), 2);
    for w in ["/w0", "/w1"] {
        assert_eq!(r.entity(w).props.get("hp").and_then(Json::as_f64), Some(40.0), "{w}: {:?}", r.events("facultas_refused"));
    }
    assert!(r.events("facultas_refused").is_empty());
}

/// Review of m5-gmods (major 2): a cast refused because the call's sound budget ran out (the
/// 17th flare in one call) changes nothing: the caster isn't charged, its wisp isn't hit.
#[test]
fn a_refused_cast_changes_nothing() {
    let mut r = Run::new();
    casters(&mut r, 17);
    let before = r.entity("/c16").props.clone();
    r.host.load(ModuleCode::of::<Requests<17>>(), Default::default(), None, &mut r.world, 0, HZ, &mut no_journal()).unwrap();
    r.ticks(3);
    for i in 0..16 {
        assert_eq!(r.entity(&format!("/w{i}")).props.get("hp").and_then(Json::as_f64), Some(40.0), "w{i}");
    }
    let refused = r.events("facultas_refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].get("code").and_then(Json::as_str), Some("too_many_sounds"));
    assert_eq!(r.entity("/c16").props, before, "the refused caster's props are byte-identical");
    assert_eq!(r.entity("/w16").props.get("hp").and_then(Json::as_f64), Some(100.0));
    assert!(r.host.running("facultas"));
}

/// Review of m5-gmods (major 3): 256 defeated wisps far away used to fill the target cap before
/// the reach filter, so a live wisp 2 m away was never hit. Targets come from the tag index
/// within reach, and a defeated target loses its tag.
#[test]
fn damage_reaches_live_targets_past_256_others() {
    let mut r = Run::new();
    for i in 0..300 {
        r.wisp(&format!("far{i}"), 500.0 + i as f32, 100);
    }
    r.wisp("near", 2.0, 100);
    r.press("ability_1");
    assert_eq!(r.entity("/near").props.get("hp").and_then(Json::as_f64), Some(40.0));
    assert_eq!(r.entity("/far0").props.get("hp").and_then(Json::as_f64), Some(100.0), "out of reach");
    // Defeated: untagged (tagged `defeated`), so it never counts toward a cap again.
    r.wisp("weak", 1.0, 10);
    r.ticks(90);
    r.press("ability_1");
    let weak = r.entity("/weak");
    assert!(!weak.tags.contains("wisp") && weak.tags.contains("defeated"), "{:?}", weak.tags);
}

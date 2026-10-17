//! Motion checks catch what they claim to: sliding feet, hovering, penetration, broken joint
//! limits, cuts at transitions, root drift; rules come from TOML, exemptions are precise.

use specula_anim::checks::{self, MotionRules};
use specula_anim::graph::Value;
use specula_anim::play::Graphed;
use specula_anim::{AnimErrorCode, AnimSet, Skeleton};
use specula_model::{Bone, Rig};

fn rig() -> Rig {
    let b = |name: &str, parent: Option<usize>, head: [f32; 3], tail: [f32; 3]| Bone { name: name.into(), parent, head, tail };
    Rig {
        bones: vec![
            b("root", None, [0.0; 3], [0.0; 3]),
            b("hips", Some(0), [0.0, 1.0, 0.0], [0.0, 1.2, 0.0]),
            b("thigh_l", Some(1), [0.15, 1.0, 0.0], [0.15, 0.55, 0.05]),
            b("shin_l", Some(2), [0.15, 0.55, 0.05], [0.15, 0.06, 0.0]),
            b("foot_l", Some(3), [0.15, 0.06, 0.0], [0.15, 0.0, 0.2]),
            b("thigh_r", Some(1), [-0.15, 1.0, 0.0], [-0.15, 0.55, 0.05]),
            b("shin_r", Some(5), [-0.15, 0.55, 0.05], [-0.15, 0.06, 0.0]),
            b("foot_r", Some(6), [-0.15, 0.06, 0.0], [-0.15, 0.0, 0.2]),
            b("chest", Some(1), [0.0, 1.2, 0.0], [0.0, 1.6, 0.0]),
            b("arm", Some(8), [0.3, 1.5, 0.0], [0.3, 0.9, 0.0]),
        ],
    }
}

const LEGS: &str = r#""pelvis": "hips", "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" }, { "thigh": "thigh_r", "shin": "shin_r", "foot": "foot_r", "phase": 0.5 } ]"#;

fn set() -> AnimSet {
    let text = format!(
        r#"{{ "anims": 1, "clips": [
        {{ "name": "walk", "kind": "gait", "duration": 1.0, "stride": 0.7, {LEGS} }},
        {{ "name": "skate", "kind": "clip", "duration": 1, "loop": "loop", "distance": 0.8, "tracks": [
           {{ "bone": "chest", "channel": "rotation", "times": [0, 1], "values": [[0, 0, 0], [0, 0, 0]] }} ] }},
        {{ "name": "hover", "kind": "clip", "duration": 1, "loop": "loop", "tracks": [
           {{ "bone": "root", "channel": "translation", "times": [0, 1], "values": [[0, 0.2, 0], [0, 0.2, 0]] }} ] }},
        {{ "name": "dig", "kind": "keys", "duration": 1, "keys": [ {{ "time": 0, "pose": {{}} }}, {{ "time": 1, "pose": {{ "arm": {{ "move": [0, -1.2, 0] }} }} }} ] }},
        {{ "name": "kneel", "kind": "pose", "pose": {{ "shin_l": {{ "rot": [-40, 0, 0] }} }} }},
        {{ "name": "raise", "kind": "pose", "pose": {{ "arm": {{ "rot": [0, 0, 150] }} }} }} ],
      "graphs": {{ "g": {{ "params": {{ "act": {{ "type": "enum", "values": ["walk", "raise"] }}, "speed": {{ "type": "float", "min": 0, "max": 5 }}, "cut": "bool" }},
          "states": {{ "walk": {{ "play": "walk", "match_speed": "speed" }}, "raise": "raise" }},
          "transitions": [ {{ "from": "walk", "to": "raise", "when": "act == raise and cut", "blend": 0 }},
                           {{ "from": "walk", "to": "raise", "when": "act == raise", "blend": 0.4 }} ],
          "limits": {{ "shin_l": {{ "x": [-5, 150] }} }} }} }} }}"#
    );
    AnimSet::load(&text, Skeleton::from_rig(&rig()).unwrap()).unwrap_or_else(|e| panic!("{e}"))
}

fn find<'a>(r: &'a checks::Report, check: &str) -> &'a checks::Check {
    r.checks.iter().find(|c| c.check == check).unwrap_or_else(|| panic!("no {check} in {:?}", r.checks))
}

#[test]
fn clips_are_measured() {
    let s = set();
    let rules = MotionRules::default();
    // A planted gait: no sliding, on the ground, seamless.
    let walk = checks::check_clip(&s, "t", s.id("walk").unwrap(), &rules, None);
    assert!(walk.passed(), "{:?}", walk.failures().collect::<Vec<_>>());
    assert!(find(&walk, "foot_sliding").value < 1.0);
    // A rest pose travelling 0.8 m per second skates both feet at 80 cm/s.
    let skate = checks::check_clip(&s, "t", s.id("skate").unwrap(), &rules, None);
    let fs = find(&skate, "foot_sliding");
    assert!(!fs.passed && (fs.value - 80.0).abs() < 1.0, "{fs:?}");
    assert!(fs.error_json().unwrap().get("error").is_some());
    // Lifted 20 cm off the ground: hovering.
    let hover = checks::check_clip(&s, "t", s.id("hover").unwrap(), &rules, None);
    let fl = find(&hover, "floating");
    assert!(!fl.passed && (fl.value - 20.0).abs() < 0.5, "{fl:?}");
    // An arm swung down through the floor; a knee bent backwards past its limit.
    let dig = checks::check_clip(&s, "t", s.id("dig").unwrap(), &rules, None);
    let gp = find(&dig, "ground_penetration");
    assert!(!gp.passed && gp.bone.as_deref() == Some("arm"), "{gp:?}");
    let kneel = checks::check_clip(&s, "t", s.id("kneel").unwrap(), &rules, None);
    let jl = find(&kneel, "joint_limits");
    assert!(!jl.passed && (jl.value - 35.0).abs() < 0.5 && jl.bone.as_deref() == Some("shin_l"), "{jl:?}");
    // The report as data: failures first, bounded.
    let j = skate.to_json(2);
    assert_eq!(j.get("passed"), Some(&specula_core::Json::Bool(false)));
    assert!(j.get("checks_omitted").is_some());
}

#[test]
fn cuts_pop_and_blends_do_not() {
    let s = set();
    let rules = MotionRules::default();
    let g = Graphed { set: &s, graph: s.graph("g").unwrap(), scale: 1.0 };
    let walk = vec![("act".to_string(), Value::Name("walk".into())), ("speed".to_string(), Value::Num(0.7))];
    let mut raise = walk.clone();
    raise[0].1 = Value::Name("raise".into());
    let blended = checks::check_transition(g, "t", "g", &walk, &raise, 0.5, &rules);
    let pop = find(&blended, "pops");
    assert!(pop.passed, "a 0.4 s blend: {pop:?}");
    let mut cut = raise.clone();
    cut.push(("cut".to_string(), Value::Bool(true)));
    let cut_r = checks::check_transition(g, "t", "g", &walk, &cut, 0.5, &rules);
    let pop = find(&cut_r, "pops");
    assert!(!pop.passed && pop.value > 15.0, "a cut: {pop:?}");
    // A state held by its parameters, with travel at its match_speed: planted.
    let r = checks::check_state(g, "t", "g", "walk", &walk, 3.0, &rules, None).unwrap();
    assert!(r.passed() && r.notes.is_empty(), "{:?}", r.failures().collect::<Vec<_>>());
    // Parameters that move the state on are reported.
    let r = checks::check_state(g, "t", "g", "walk", &raise, 3.0, &rules, None).unwrap();
    assert!(r.notes[0].contains("moves on to 'raise'"), "{:?}", r.notes);
    assert_eq!(checks::check_state(g, "t", "g", "swim", &walk, 1.0, &rules, None).unwrap_err().code, AnimErrorCode::UnknownName);
}

#[test]
fn rules_from_toml_and_precise_exemptions() {
    let r = MotionRules::from_toml(
        r#"
[motion]
foot_sliding_cm_s = 2.5
pops_cm = 3

[motion.allow]
"ogre/*walk" = ["foot_sliding:foot_r", "pops"]
"ogre/death" = ["ground_penetration"]
"#,
        "test.toml",
    )
    .unwrap();
    assert_eq!((r.foot_sliding_cm_s, r.pops_cm, r.contact_m), (2.5, 3.0, 0.025));
    assert!(r.allows("ogre/creature.walk", "foot_sliding", Some("foot_r")));
    assert!(!r.allows("ogre/creature.walk", "foot_sliding", Some("foot_l")), "only that foot");
    assert!(!r.allows("ogre/creature.walk", "foot_sliding", None));
    assert!(r.allows("ogre/walk", "pops", None) && !r.allows("ogre/walker", "pops", None));
    assert!(r.allows("ogre/death", "ground_penetration", None) && !r.allows("ogre/deathly", "ground_penetration", None));
    let e = MotionRules::from_toml("[motion]\nfoot_slide = 1\n", "x").unwrap_err();
    assert_eq!(e.code, AnimErrorCode::UnknownField);
    let e = MotionRules::from_toml("[motion.allow]\n\"a/b\" = [\"sliding\"]\n", "x").unwrap_err();
    assert_eq!(e.code, AnimErrorCode::UnknownName);
    assert_eq!(MotionRules::from_toml("[other]\nx = 1\n", "x").unwrap().foot_sliding_cm_s, 6.0, "no [motion]: defaults");
    // Exempt checks still report, passing, marked exempt.
    let s = set();
    let skate_rules = MotionRules::from_toml("[motion.allow]\n\"t/skate\" = [\"foot_sliding:foot_l\"]\n", "x").unwrap();
    let rep = checks::check_clip(&s, "t", s.id("skate").unwrap(), &skate_rules, None);
    let fs = find(&rep, "foot_sliding");
    assert_eq!(fs.bone.as_deref(), Some("foot_r"), "the exempt foot is left out");
    assert!(fs.detail.iter().any(|(k, _)| *k == "exempt_feet"));
}

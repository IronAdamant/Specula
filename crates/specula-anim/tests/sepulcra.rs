//! The Sepulcra creatures: rigs + anim sets load, clips are sane, feet stay planted, skinning
//! the bind pose gives the rest mesh, every clip, graph state and transition passes the motion
//! checks of `style.toml`, and sampling is bit-identical (pinned).

use std::collections::BTreeMap;
use std::path::PathBuf;

use specula_anim::checks::{self, MotionRules, Report};
use specula_anim::graph::{ParamKind, Value};
use specula_anim::play::Graphed;
use specula_anim::{measure, skin_mesh, AnimSet, Skeleton};
use specula_core::hash::StableHasher;
use specula_model::{build, BuildParams, Model, Style};

const CREATURES: [&str; 5] = ["shambler", "skeleton_runner", "wraith", "bone_colossus", "brazier_fire"];

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets")
}

fn creature(name: &str) -> (Model, AnimSet) {
    let style = Style::parse(&std::fs::read_to_string(assets().join("style.json")).unwrap()).unwrap();
    let mut lib = BTreeMap::new();
    for e in std::fs::read_dir(assets().join("recipes")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
    }
    let model = build(&lib[name], &BuildParams { library: Some(&lib), ..BuildParams::new(&style) }).unwrap();
    let skel = Skeleton::from_rig(model.rig.as_ref().unwrap()).unwrap();
    let set = AnimSet::load(&std::fs::read_to_string(assets().join(format!("anims/{name}.json"))).unwrap(), skel).unwrap_or_else(|e| panic!("{name}: {e}"));
    (model, set)
}

#[test]
fn every_anim_file_has_a_creature() {
    for e in std::fs::read_dir(assets().join("anims")).unwrap() {
        let stem = e.unwrap().path().file_stem().unwrap().to_string_lossy().to_string();
        assert!(CREATURES.contains(&stem.as_str()), "anims/{stem}.json is not tested");
    }
}

#[test]
fn creature_clips_are_complete() {
    let expect: &[(&str, &[&str])] = &[
        ("shambler", &["idle", "walk", "attack", "death", "shuffle", "still", "flinch"]),
        ("skeleton_runner", &["idle", "sprint", "jog", "leap", "death", "still", "flinch"]),
        ("wraith", &["float", "swoop", "death", "float_shroud", "swoop_shroud", "still", "flinch"]),
        ("bone_colossus", &["walk", "lumber", "idle", "summon", "death", "still", "flinch"]),
        ("brazier_fire", &["flicker"]),
    ];
    for (name, clips) in expect {
        let (_, set) = creature(name);
        assert_eq!(set.rig.as_deref(), Some(*name));
        let have: Vec<&str> = set.clips().map(|(_, c)| c.name.as_str()).collect();
        assert_eq!(&have, clips, "{name}");
        for (id, c) in set.clips() {
            c.validate().unwrap();
            if let Some(seam) = measure::loop_seam(&set, id) {
                assert!(seam < 1e-5, "{name}/{}: loop seam {seam} m", c.name);
            }
        }
    }
    // Gameplay events the sim will listen for.
    let event = |name: &str, clip: &str, ev: &str| {
        let (_, set) = creature(name);
        let c = set.clip(set.id(clip).unwrap()).clone();
        c.events.iter().filter(|e| e.name == ev).count()
    };
    assert_eq!(event("shambler", "walk", "footstep"), 2);
    assert_eq!(event("shambler", "attack", "hit"), 1);
    assert_eq!(event("skeleton_runner", "leap", "takeoff"), 1);
    assert_eq!(event("skeleton_runner", "leap", "land"), 1);
    assert_eq!(event("wraith", "swoop", "strike"), 1);
    assert_eq!(event("wraith", "death", "vanish"), 1);
    assert_eq!(event("bone_colossus", "walk", "stomp"), 2);
    assert_eq!(event("bone_colossus", "summon", "summon"), 1);
    assert_eq!(event("bone_colossus", "death", "impact"), 1);
}

/// (creature, entity scale in the game, lane speed in m/s): as the game plays them.
const WALKERS: [(&str, f32, f32); 4] = [("shambler", 1.2, 0.8), ("skeleton_runner", 1.2, 2.2), ("wraith", 1.2, 1.4), ("bone_colossus", 1.0, 0.4)];

fn rules() -> MotionRules {
    MotionRules::from_toml(&std::fs::read_to_string(assets().join("style.toml")).unwrap(), "style.toml").unwrap()
}

#[test]
fn creature_graphs_share_one_interface() {
    // The game drives every creature the same way: these parameters, a state per act.
    for (name, _, _) in WALKERS {
        let (_, set) = creature(name);
        let g = set.graph("creature").unwrap_or_else(|| panic!("{name}: no 'creature' graph"));
        let kind = |p: &str| g.params[g.param(p).unwrap_or_else(|| panic!("{name}: no param {p}")) as usize].kind.clone();
        assert_eq!(kind("act"), ParamKind::Enum(["idle", "move", "attack", "special", "death"].map(String::from).to_vec()), "{name}");
        assert!(matches!(kind("speed"), ParamKind::Float { .. }) && matches!(kind("look"), ParamKind::Float { .. }));
        assert_eq!((kind("hit"), kind("look_at")), (ParamKind::Trigger, ParamKind::Vec3));
        for act in ["idle", "move", "attack", "special", "death"] {
            assert!(g.layers[0].state(act).is_some(), "{name}: no state '{act}'");
        }
        assert!(g.layers.iter().any(|l| l.name == "flinch"), "{name}: hits flinch");
    }
}

#[test]
fn creature_motion_passes_its_style_rules() {
    let rules = rules();
    assert_eq!(rules.contact_m, 0.025, "the research's 2.5 cm contact rule");
    let mut all = Report::default();
    for (name, scale, speed) in WALKERS {
        let (model, set) = creature(name);
        let mesh = &model.lods[0];
        for (id, _) in set.clips() {
            all.checks.extend(checks::check_clip(&set, name, id, &rules, Some(mesh)).checks);
        }
        let g = Graphed { set: &set, graph: set.graph("creature").unwrap(), scale };
        let act = |a: &str, v: f32| vec![("act".to_string(), Value::Name(a.into())), ("speed".to_string(), Value::Num(v))];
        for state in ["idle", "move", "attack", "special", "death"] {
            let r = checks::check_state(g, name, "creature", state, &act(state, speed), 3.0, &rules, Some(mesh)).unwrap();
            assert!(r.notes.is_empty(), "{name}.{state}: {:?}", r.notes);
            all.checks.extend(r.checks);
        }
        // Slowed by frost or runes: the slow end of the locomotion blend.
        all.checks.extend(checks::check_state(g, name, "creature", "move", &act("move", speed * 0.4), 3.0, &rules, Some(mesh)).unwrap().checks);
        for (a, b) in [("idle", "move"), ("move", "idle"), ("move", "attack"), ("attack", "move"), ("move", "special"), ("move", "death"), ("attack", "death")] {
            all.checks.extend(checks::check_transition(g, name, "creature", &act(a, speed), &act(b, speed), 1.37, &rules).checks);
        }
    }
    let failures: Vec<String> = all.failures().map(|c| format!("{} {} {:.2} {} (limit {}) {:?}", c.subject, c.check, c.value, c.unit, c.limit, c.bone)).collect();
    assert!(failures.is_empty(), "motion checks failed:\n{}", failures.join("\n"));
    // The checks really ran: feet were found in contact, and every kind of check appears.
    for check in checks::CHECKS {
        assert!(all.checks.iter().any(|c| c.check == check), "no '{check}' check ran");
    }
    let walk = all.checks.iter().find(|c| c.subject == "shambler/creature.move" && c.check == "foot_sliding").unwrap();
    assert!(walk.detail.iter().any(|(k, v)| *k == "contact" && *v > 0.5), "{walk:?}");
}

#[test]
fn locomotion_keeps_feet_planted() {
    // (creature, clip, max slide m/s, gait duty): contact must be found for most of the stance,
    // so a slide of ~0 is a real measurement, not an empty one.
    for (name, clip, max, duty) in [("shambler", "walk", 0.06, 0.65), ("skeleton_runner", "sprint", 0.12, 0.38), ("bone_colossus", "walk", 0.06, 0.7)] {
        let (_, set) = creature(name);
        let id = set.id(clip).unwrap();
        let feet = [set.skeleton.find("foot_l").unwrap(), set.skeleton.find("foot_r").unwrap()];
        let m = measure::foot_contact(&set, id, &feet, 0.001, 480);
        assert!(m.max_slide < max, "{name}/{clip}: feet slide {} m/s during contact", m.max_slide);
        assert!(m.contact > 0.8 * duty && m.contact <= duty + 0.01, "{name}/{clip}: contact {} of the cycle, stance {duty}", m.contact);
        assert!(set.clip(id).distance > 0.0);
    }
}

#[test]
fn bind_pose_skins_to_the_rest_mesh_and_deaths_end_on_the_ground() {
    for name in CREATURES {
        let (model, set) = creature(name);
        let s = &set.skeleton;
        let (mut m, mut sk) = (Vec::new(), Vec::new());
        s.model_space(&s.rest_pose(), &mut m);
        s.skin_matrices(&m, &mut sk);
        for (i, lod) in model.lods.iter().enumerate() {
            let out = skin_mesh(lod, &sk);
            let worst = out.positions.iter().zip(&lod.positions).map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).abs()).fold(0.0f32, f32::max)).fold(0.0, f32::max);
            assert!(worst < 1e-5, "{name} LOD{i}: bind pose moves vertices by {worst} m");
        }
        // Every clip keeps every joint finite and near the creature (nothing explodes).
        let h = model.bounds.size()[1];
        for (id, c) in set.clips() {
            for k in 0..=10 {
                let t = c.duration * k as f32 / 10.0;
                for p in &measure::joints_at(&set, id, t) {
                    assert!(p.iter().all(|v| v.is_finite() && v.abs() < 4.0 * h + 4.0), "{name}/{} at {t}: joint {p:?}", c.name);
                }
            }
        }
    }
    // Deaths finish low: the colossus lies down, the runner's bones rest on the floor.
    let (model, set) = creature("bone_colossus");
    let death = set.id("death").unwrap();
    let head = set.skeleton.find("head").unwrap();
    let end = measure::joints_at(&set, death, set.clip(death).duration);
    assert!(end[head][1] < 0.3 * model.bounds.size()[1], "colossus head ends at {} m", end[head][1]);
    let (_, set) = creature("skeleton_runner");
    let death = set.id("death").unwrap();
    let end = measure::joints_at(&set, death, set.clip(death).duration);
    assert!(end.iter().skip(1).all(|p| p[1] > 0.0 && p[1] < 0.5), "{end:?}");
}

#[test]
fn sampled_poses_are_bit_identical_pinned() {
    // One hash over every creature clip's poses every 10 ms. If it changes, the motion changed:
    // check that was intended (anim edit, generator or maths change), then update the pin.
    let mut h = StableHasher::new();
    for name in CREATURES {
        let (_, set) = creature(name);
        let (_, again) = creature(name);
        for (id, c) in set.clips() {
            let a = measure::pose_hash(&set, id, 0.01).unwrap();
            assert_eq!(a, measure::pose_hash(&again, id, 0.01).unwrap(), "{name}/{}: two loads differ", c.name);
            h.write_str(&c.name);
            h.write_u64(a);
        }
    }
    assert_eq!(format!("{:016x}", h.finish()), PINNED, "Sepulcra creature motion changed");
}

const PINNED: &str = "3141c037f0c6c29f";

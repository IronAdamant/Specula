//! Motion checks for every Sepulcra creature: each clip, each graph state, and the transitions
//! between acts, against `games/sepulcra/assets/style.toml`'s `[motion]` rules. Prints one line
//! per check (failures first) and exits 1 if any failed.
//!
//! ```text
//! cargo run -p specula-anim --example motion_checks --release -- [--only shambler] [--all]
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use specula_anim::checks::{self, MotionRules, Report};
use specula_anim::graph::Value;
use specula_anim::play::Graphed;
use specula_anim::{AnimSet, Skeleton};
use specula_model::{build, BuildParams, Model, Style};

const CREATURES: [(&str, f32, f32); 4] = [("shambler", 1.2, 0.8), ("skeleton_runner", 1.2, 2.2), ("wraith", 1.2, 1.4), ("bone_colossus", 1.0, 0.4)];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let only = args.iter().position(|a| a == "--only").and_then(|i| args.get(i + 1)).cloned();
    let all = args.iter().any(|a| a == "--all");
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    let style = Style::parse(&std::fs::read_to_string(assets.join("style.json")).unwrap()).unwrap();
    let rules = match std::fs::read_to_string(assets.join("style.toml")) {
        Ok(t) => MotionRules::from_toml(&t, "games/sepulcra/assets/style.toml").unwrap_or_else(|e| panic!("{e}")),
        Err(_) => MotionRules::default(),
    };
    let mut library = BTreeMap::new();
    for e in std::fs::read_dir(assets.join("recipes")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            library.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
    }
    let mut failed = 0;
    for (name, scale, speed) in CREATURES {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let model: Model = build(&library[name], &BuildParams { library: Some(&library), ..BuildParams::new(&style) }).unwrap();
        let skel = Skeleton::from_rig(model.rig.as_ref().unwrap()).unwrap();
        let set = AnimSet::load(&std::fs::read_to_string(assets.join(format!("anims/{name}.json"))).unwrap(), skel).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mesh = &model.lods[0];
        if args.iter().any(|a| a == "--rig") {
            for (i, n) in set.skeleton.names.iter().enumerate() {
                println!("{name} bone {n:<12} head {:?} tail {:?}", set.skeleton.heads[i], set.skeleton.tails[i]);
            }
            println!("{name} mesh lowest at rest {:.3}", mesh.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min));
        }
        let mut report = Report::default();
        for (id, _) in set.clips() {
            report.checks.extend(checks::check_clip(&set, name, id, &rules, Some(mesh)).checks);
        }
        if let Some(graph) = set.graph("creature") {
            let g = Graphed { set: &set, graph, scale };
            let act = |a: &str| vec![("act".to_string(), Value::Name(a.into())), ("speed".to_string(), Value::Num(speed))];
            for state in ["idle", "move", "attack", "special", "death"] {
                let r = checks::check_state(g, name, "creature", state, &act(state), 3.0, &rules, Some(mesh)).unwrap();
                report.notes.extend(r.notes);
                report.checks.extend(r.checks);
            }
            // Slowed: the shuffle end of the blend.
            let slow = vec![("act".to_string(), Value::Name("move".into())), ("speed".to_string(), Value::Num(speed * 0.4))];
            let r = checks::check_state(g, &format!("{name}(slowed)"), "creature", "move", &slow, 3.0, &rules, Some(mesh)).unwrap();
            report.checks.extend(r.checks);
            for (a, b) in [("idle", "move"), ("move", "idle"), ("move", "attack"), ("attack", "move"), ("move", "special"), ("move", "death"), ("attack", "death")] {
                report.checks.extend(checks::check_transition(g, name, "creature", &act(a), &act(b), 1.37, &rules, Some(mesh)).checks);
            }
        }
        let mut lines: Vec<_> = report.checks.iter().filter(|c| all || !c.passed || c.check == "foot_sliding" || c.check == "pops").collect();
        lines.sort_by_key(|c| c.passed);
        for c in lines {
            let detail: Vec<String> = c.detail.iter().map(|(k, v)| format!("{k}={v:.2}")).collect();
            println!(
                "{:4} {:<44} {:<18} {:>8.2} {:<6} (limit {:>6.2}) {} {}{}",
                if c.passed { if c.exempt { "exmp" } else { "ok" } } else { "FAIL" },
                c.subject,
                c.check,
                c.value,
                c.unit,
                c.limit,
                c.bone.as_deref().unwrap_or(""),
                c.at_s.map_or(String::new(), |t| format!("at {t:.2}s ")),
                detail.join(" ")
            );
        }
        for n in &report.notes {
            println!("note {name}: {n}");
        }
        failed += report.failures().count();
    }
    println!("rules: {}", rules.to_json());
    if failed > 0 {
        println!("{failed} checks failed");
        std::process::exit(1);
    }
}

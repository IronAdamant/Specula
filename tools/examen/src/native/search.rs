//! `search_check`: what the engine looks for itself, through the command an agent with a shell
//! uses (docs/design/FIRST_HOUR.md, section 15). A project made by `specula new` is given a
//! small game written whole (a ball on a board, three coins, pushes of a strength the rules
//! read with `Cx::param`), and then:
//!
//! - a run is read on its way (`--at`), with the ball's place from the world itself;
//! - a run is played for each of three values of the push (`--set push=2,4,8`), and the ball
//!   goes further with more;
//! - the push at which the ball passes a line is looked for (`specula tune`), and found
//!   between the two values the last line names;
//! - a script that takes all three coins is looked for (`specula find`), found, and said to
//!   have been played again from the start;
//! - **that script, in a run of its own, ends in the state the search answered, to the last
//!   bit**: a game put back from a save plays on as the game that was saved. This is the
//!   proof the search rests on, kept as a check;
//! - the same search again finds the same script.

use specula_core::Json;

use super::{at, expect, text, Cx};

const RULES: &str = r##"#![deny(unsafe_code)]
use specula_sdk::{Cx, Module, ModuleInfo, SdkError, Spawn, StopReason, Vec3};

const ROOT: &str = "/nummi";
const COINS: [(f32, f32); 3] = [(2.0, 0.0), (-2.0, 2.0), (0.0, -3.0)];

#[derive(Default)]
pub struct Rules;
specula_sdk::export_module!(Rules);

impl Module for Rules {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo { name: "nummi", version: env!("CARGO_PKG_VERSION"), description: "A ball takes three coins.", order: 0, actions: include_str!("../actions.json"), hooks: Vec::new() }
    }
    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        if cx.find(ROOT)?.is_none() {
            cx.spawn(Spawn::new("group").name("nummi"))?;
            cx.spawn(Spawn::new("cube").name("board").parent(ROOT).position(Vec3::new(0.0, -0.1, 0.0)).scale(Vec3::new(8.0, 0.2, 8.0)).collider_box(Vec3::new(1.0, 1.0, 1.0)))?;
            cx.spawn(Spawn::new("sphere").name("ball").parent(ROOT).position(Vec3::new(0.0, 0.25, 0.0)).scale(Vec3::new(0.5, 0.5, 0.5)).collider_sphere(0.5).dynamic(1.0))?;
            for (x, z) in COINS {
                cx.spawn(Spawn::new("sphere").parent(ROOT).position(Vec3::new(x, 0.25, z)).scale(Vec3::new(0.3, 0.3, 0.3)).color("#ffd24a").tags(&["coin"]))?;
            }
            cx.set_state("score", 0u64)?;
            cx.set_state("won", false)?;
        }
        Ok(())
    }
    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        // The tick the run begins in is the engine's telling of it, and no push: a game put back
        // from a save of its start must still be told (a save that lost what the rules had yet
        // to hear put this ball a tick ahead of a run of its own, 2026-11-22).
        if cx.events().iter().any(|e| e.from == "engine" && e.name == "run") {
            return Ok(());
        }
        let ball = cx.find("/nummi/ball")?.expect("ball");
        let a = cx.param("push", 5.6)? as f32 * cx.dt();
        let mut push = Vec3::new(0.0, 0.0, 0.0);
        if cx.action("right").held { push.x += a; }
        if cx.action("left").held { push.x -= a; }
        if cx.action("down").held { push.z += a; }
        if cx.action("up").held { push.z -= a; }
        if push.x != 0.0 || push.z != 0.0 {
            cx.push(ball, push)?;
        }
        let p = cx.entity(ball)?.expect("ball").position();
        let mut score = cx.state("score")?.as_u64().unwrap_or(0);
        for coin in cx.query(Some("coin"), None, 16)? {
            let c = cx.entity(coin)?.expect("coin").position();
            let (dx, dy, dz) = (c.x - p.x, c.y - p.y, c.z - p.z);
            if dx * dx + dy * dy + dz * dz < 0.25 {
                cx.delete(coin)?;
                score += 10;
            }
        }
        cx.set_state("score", score)?;
        cx.set_state("won", score >= 30)?;
        Ok(())
    }
    fn stop(&self, _: &mut Cx, _: StopReason) -> Result<(), SdkError> {
        Ok(())
    }
}
"##;

const ACTIONS: &str = r#"{
  "format": "specula.input.v1",
  "actions": [
    {"name": "right", "kind": "button", "group": "move", "description": "push right", "bindings": ["Key.D"]},
    {"name": "left", "kind": "button", "group": "move", "description": "push left", "bindings": ["Key.A"]},
    {"name": "up", "kind": "button", "group": "move", "description": "push away", "bindings": ["Key.W"]},
    {"name": "down", "kind": "button", "group": "move", "description": "push near", "bindings": ["Key.S"]}
  ]
}
"#;

const GAME: &str = r#"{
  "format": "specula.game.v1",
  "title": "nummi",
  "tick_hz": 60,
  "start": "run",
  "modules": ["modules/nummi"],
  "inputs": { "east": "right" },
  "report": { "tick": "run.ticks", "score": "nummi.score", "won": "nummi.won", "ball": "world./nummi/ball.position", "coins": "world.#coin.count" }
}
"#;

/// A number of a state, by its path.
fn number(json: &Json, path: &str) -> f64 {
    at(json, path).and_then(Json::as_f64).unwrap_or(f64::NAN)
}

pub fn search(cx: &mut Cx) -> Result<String, String> {
    let settings = cx.settings();
    let projects = cx.out.join("p");
    let project = projects.join("nummi");
    if project.exists() {
        return Err(format!("{} is there already: give the check an output folder of its own (it never deletes one)", project.display()));
    }
    let (code, made) = cx.cli(&["new", "nummi", "--in", &projects.to_string_lossy(), "--settings", &settings])?;
    expect(code == 0, "`specula new` did not make the project", &made)?;
    let write = |file: &str, text: &str| std::fs::write(file.split('/').fold(project.clone(), |p, part| p.join(part)), text).map_err(|e| format!("{file}: {e}"));
    write("modules/nummi/src/lib.rs", RULES)?;
    write("modules/nummi/actions.json", ACTIONS)?;
    write("game.json", GAME)?;
    // (The sample's own checks are the tap game's.)
    std::fs::remove_dir_all(project.join("checks")).map_err(|e| format!("checks: {e}"))?;
    let folder = project.to_string_lossy().into_owned();
    let run = |cx: &mut Cx, words: &[&str]| {
        let mut all: Vec<&str> = words.to_vec();
        all.extend(["--settings", &settings]);
        cx.cli_lines(&all)
    };

    // 1. A run read on its way, in the game's own word, the ball's place from the world.
    let (code, lines) = run(cx, &["run", &folder, "--at", "30,60", "--inputs", "0:east", "--keys", "tick,ball,coins"])?;
    expect(code == 0 && lines.len() == 2, "a run with --at 30,60 did not print two states", &Json::Arr(lines.clone()))?;
    let (first, second) = (&lines[0], &lines[1]);
    expect(number(first, "tick") == 30.0 && number(second, "tick") == 60.0 && number(first, "ball.x") > 0.3 && number(second, "ball.x") > number(first, "ball.x") && at(first, "score").is_none(), "the states on the way are not the two ticks, with the ball further at the second and only the lines asked for", &Json::Arr(lines.clone()))?;
    expect(number(first, "coins") == 3.0 && number(second, "coins") == 2.0, "the coin at (2, 0) was not taken between tick 30 and tick 60", &Json::Arr(lines.clone()))?;

    // 2. A run for each value of the push.
    let (code, lines) = run(cx, &["run", &folder, "--ticks", "30", "--inputs", "0:right", "--set", "push=2,4,8", "--keys", "ball"])?;
    let xs: Vec<f64> = lines.iter().map(|l| number(l, "state.ball.x")).collect();
    expect(code == 0 && lines.len() == 3 && xs[0] < xs[1] && xs[1] < xs[2] && number(&lines[2], "set.push") == 8.0, "a run for each of three pushes is not three lines, the ball further with more push", &Json::Arr(lines.clone()))?;

    // 3. The push at which the ball is past x = 1 after thirty ticks.
    let (code, lines) = run(cx, &["tune", &folder, "--ticks", "30", "--inputs", "0:right", "--vary", "push=0..20", "--until", "ball.x>1", "--tries", "7", "--keys", "ball"])?;
    let last = lines.last().cloned().unwrap_or(Json::Null);
    let (holds, fails) = (number(&last, "holds_at"), number(&last, "fails_at"));
    expect(code == 0 && lines.len() == 8 && at(&last, "found").and_then(Json::as_bool) == Some(true) && fails < holds && holds - fails < 1.0, "the tuning did not close in on where the condition begins to hold", &Json::Arr(lines.clone()))?;
    // (What it found is what a run of that value does.)
    let holds_text = format!("push={holds}");
    let (_, at_holds) = run(cx, &["run", &folder, "--ticks", "30", "--inputs", "0:right", "--set", &holds_text, "--keys", "ball"])?;
    expect(at_holds.first().is_some_and(|s| number(s, "ball.x") > 1.0), "a run with the push the tuning found does not put the ball past x = 1", &Json::Arr(at_holds.clone()))?;
    let (code, neither) = run(cx, &["tune", &folder, "--ticks", "30", "--inputs", "0:right", "--vary", "push=0..1", "--until", "ball.x>1", "--tries", "5"])?;
    expect(code == 1 && neither.len() == 3 && at(&neither[2], "found").and_then(Json::as_bool) == Some(false), "a range the condition holds nowhere in is not said so after its two ends", &Json::Arr(neither.clone()))?;

    // 4. A script that takes the three coins: found, played again, and the same in a run of
    // its own.
    let find: [&str; 17] = ["find", &folder, "--ticks", "1800", "--until", "won==true", "--words", "right,left,up,down,none", "--rise", "score", "--cells", "score,ball.x,ball.z", "--never", "ball.y<0", "--tries", "12000", "--seed"];
    let search = |cx: &mut Cx, seed: &str| {
        let mut words = find.to_vec();
        words.push(seed);
        run(cx, &words)
    };
    let (code, found) = search(cx, "1")?;
    let found = found.last().cloned().unwrap_or(Json::Null);
    expect(code == 0 && at(&found, "found").and_then(Json::as_bool) == Some(true) && at(&found, "replayed").and_then(Json::as_bool) == Some(true) && number(&found, "state.score") == 30.0, "no script that takes the three coins was found and played again", &found)?;
    let (inputs, ticks) = (text(&found, "inputs").unwrap_or("").to_owned(), number(&found, "ticks").to_string());
    let (code, replay) = run(cx, &["run", &folder, "--ticks", &ticks, "--inputs", &inputs])?;
    let replay = replay.last().cloned().unwrap_or(Json::Null);
    expect(code == 0 && Some(&replay) == at(&found, "state"), "the script the search found, in a run of its own, does not end in the state the search answered: a game put back from a save did not play on as the game that was saved", &Json::obj([("found", found.clone()), ("run", replay.clone())]))?;
    let (_, again) = search(cx, "1")?;
    expect(again.last() == Some(&found), "the same search did not find the same script", &Json::obj([("first", found.clone()), ("again", again.last().cloned().unwrap_or(Json::Null))]))?;

    // 5. What a search cannot read is said before anything is played.
    let (code, no_line) = run(cx, &["find", &folder, "--ticks", "600", "--until", "lives==0", "--words", "right"])?;
    expect(code == 1 && text(&no_line[0], "error") == Some("unknown_line"), "a search told to reach a line the report has not is not refused by name", &Json::Arr(no_line.clone()))?;
    Ok(format!("a run read at two ticks; a run for each of three pushes; the push tuned to between {fails} and {holds}; a script of {} inputs that takes three coins by tick {ticks}, the same in a run of its own and found again", inputs.split(',').count()))
}

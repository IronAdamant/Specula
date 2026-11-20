//! What `specula new` puts into a project so that it builds and plays
//! (docs/design/FIRST_HOUR.md, slice 7).
//!
//! A new project used to be a folder, an empty `assets` and a marker. Whoever built a game in
//! it wrote its menus, its words, its settings file, its win condition and its module's
//! scaffolding first, and found the SDK somewhere (M5's measured comparison: nine or more
//! requests before the first line of the game). A new project is now a small game that runs:
//!
//! | Path | What |
//! |---|---|
//! | `game.json` | the game file: title, tick rate, the module, a setting the rules read, when a run is won or lost, the score |
//! | `assets/ui/*.json`, `assets/strings/en.json` | the shell's screens and words ([`crate::shell::BUILTIN`]), with a HUD for this game |
//! | `assets/style.json` | a plain style, so a model recipe builds on the first try |
//! | `modules/<name>/` | the game's rules: a module that builds, with one action |
//! | `Cargo.toml`, `crates/specula-sdk`, `crates/specula-core` | the workspace and this engine's SDK |
//! | `checks/runs.json` | the game's own checks: runs, and what each must print (`specula check`) |
//! | `checks/smoke.json` | a playtest that plays a run and reads how it ended, for an engine left running |
//! | `notes/brief.md`, `notes/how.md` | what this is, and how it is built and checked: given to every agent at its first call |
//!
//! The game is deliberately the smallest one with every part: tap five times before the time
//! runs out. Its builder replaces the rules and keeps the rest.
//!
//! [`files`] is pure (the texts, by path); [`write`] puts them on disk and adds the SDK.

use std::path::Path;

use specula_core::Json;

/// A module's name from a project's: lowercase letters, digits and `_`, starting with a
/// letter, at most 32. A name the screens' bindings keep for themselves (`game`, `setting`,
/// `ui`, `item`), a Rust keyword likely as a name, or nothing usable: `play`.
pub fn module_name(project: &str) -> String {
    let mut name = String::new();
    for c in project.chars() {
        if name.len() >= 32 {
            break;
        }
        if c.is_ascii_alphanumeric() {
            if name.is_empty() && c.is_ascii_digit() {
                continue;
            }
            name.push(c.to_ascii_lowercase());
        } else if !name.is_empty() && !name.ends_with('_') {
            name.push('_');
        }
    }
    let name = name.trim_end_matches('_').to_owned();
    const TAKEN: [&str; 17] = ["game", "setting", "ui", "run", "item", "engine", "world", "test", "core", "std", "self", "super", "crate", "mod", "use", "fn", "specula"];
    if name.is_empty() || TAKEN.contains(&name.as_str()) || name.starts_with("specula_") {
        "play".to_owned()
    } else {
        name
    }
}

/// A title as a JSON or TOML string's inside (the project's name, as given).
fn quoted(text: &str) -> String {
    let json = Json::from(text).to_string();
    json[1..json.len() - 1].to_owned()
}

const GAME: &str = r#"{
  "format": "specula.game.v1",
  "title": "{{title}}",
  "tick_hz": 30,
  "modules": ["modules/{{module}}"],
  "settings": [
    { "name": "difficulty", "kind": "choice", "options": ["easy", "normal", "hard"], "default": "normal", "sim": true }
  ],
  "outcome": {
    "won": { "key": "{{module}}.taps", "gte": 5 },
    "lost": { "key": "{{module}}.left", "lte": 0 },
    "score": "{{module}}.score"
  },
  "report": {
    "tick": "run.ticks",
    "screen": "ui.screen",
    "phase": "game.phase",
    "taps": "{{module}}.taps",
    "left": "{{module}}.left",
    "score": "game.score",
    "difficulty": "setting.difficulty",
    "volume": "setting.volume",
    "texts": "ui.top_texts"
  }
}
"#;

const HUD: &str = r#"{
  "format": "specula.ui.screen.v1",
  "name": "hud",
  "input": "hud",
  "layer": 0,
  "notes": "Shown while a run is under way. A screen shows what the game keeps: {{{module}}.KEY} is a value the module set with Cx::set_state (a map with msg and args is a line of the string tables); {game.score} the score the game file names; {setting.NAME} a setting. A button's action that the engine does not know (here: tap) reaches the module as the event ui {action, widget, screen} at the next tick.",
  "root": {
    "type": "box",
    "children": [
      {
        "type": "panel", "style": "panel", "anchor": "top_left", "margin": [12, 12, 12, 12], "layout": "row", "align": "center", "gap": 14,
        "children": [
          { "type": "label", "text": "@hud.taps", "role": "value" },
          { "type": "label", "text": "@hud.left", "role": "value" },
          { "type": "button", "id": "pause", "style": "compact", "text": "@hud.pause", "action": "pause" }
        ]
      },
      { "type": "button", "id": "tap", "style": "primary", "anchor": "bottom", "margin": [0, 0, 48, 0], "width": "12em", "text": "@hud.tap", "action": "tap" }
    ]
  }
}
"#;

const STYLE: &str = r##"{
  "style": "plain",
  "description": "The style a new project starts with: a small neutral palette and four materials, so a model recipe builds on the first try (recipes name these, never raw colours). Make it the game's own: change the colours first.",
  "palette": {
    "ink": "#1f232a",
    "stone": "#8c96a3",
    "paper": "#f0f2f5",
    "accent": "#5aa9ff",
    "warm": "#ffb35a",
    "leaf": "#6fbf73"
  },
  "materials": {
    "matte": { "color": "#8c96a3", "roughness": 0.85, "specular": 0.15 },
    "painted": { "color": "#5aa9ff", "roughness": 0.55, "specular": 0.3 },
    "metal": { "color": "#9aa3ad", "roughness": 0.4, "metallic": 0.9, "specular": 0.6 },
    "glow": { "color": "#ffb35a", "roughness": 0.5, "emissive": 2.0, "specular": 0.2 }
  }
}
"##;

const WORKSPACE: &str = r#"# This project's Cargo workspace: its module crates (every folder under modules/) and the
# Specula SDK they depend on (crates/, written by the engine: `specula sdk` puts it back).
[workspace]
resolver = "2"
members = ["crates/specula-core", "crates/specula-sdk", "modules/*"]
"#;

const MODULE_MANIFEST: &str = r#"[package]
name = "{{module}}"
description = "The rules of {{title_toml}}, as a Specula module"
version = "0.1.0"
edition = "2021"
publish = false

# A module is a library the engine loads (logic_build builds and reloads it while the game
# runs). It depends on the SDK only.
[lib]
crate-type = ["cdylib"]

[dependencies]
specula-sdk = { path = "../../crates/specula-sdk" }
"#;

const MODULE_ACTIONS: &str = r#"{
  "format": "specula.input.v1",
  "actions": [
    {
      "name": "tap",
      "kind": "button",
      "group": "play",
      "description": "Tap. Agents: logic_input {\"module\": \"{{module}}\", \"actions\": {\"tap\": true}}, or the HUD's button (game_act {\"action\": \"tap\"}).",
      "bindings": ["Key.Space", "Pad.South"]
    }
  ]
}
"#;

const MODULE_SOURCE: &str = r##"//! The rules of the game, as a Specula module (notes/how.md says how one is written).
//! The smallest game with every part: tap five times before the time runs out. What the game
//! remembers is in the state store and the world, never in this struct (a rebuild reloads
//! while the game runs and loses nothing); what a screen shows is what is kept there.

// `export_module!` needs one `unsafe` symbol; nothing else here may use it.
#![deny(unsafe_code)]

use specula_sdk::{Cx, Module, ModuleInfo, Patch, SdkError, Spawn, StopReason, Value, Vec3};

const ROOT: &str = "/{{module}}";
const THING: &str = "/{{module}}/thing";

#[derive(Default)]
pub struct Rules;

specula_sdk::export_module!(Rules);

/// The seconds a run lasts, by the player's difficulty.
fn seconds_for(difficulty: &str) -> u64 {
    match difficulty {
        "easy" => 15,
        "hard" => 6,
        _ => 10,
    }
}

impl Module for Rules {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo {
            name: "{{module}}",
            version: env!("CARGO_PKG_VERSION"),
            description: "Taps before the time runs out: keeps taps, left (seconds) and score (ten a tap, and the seconds left).",
            order: 0,
            actions: include_str!("../actions.json"),
            hooks: Vec::new(),
        }
    }

    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        // After a reload everything below exists already: find it, build nothing twice.
        if cx.find(ROOT)?.is_none() {
            cx.spawn(Spawn::new("group").name("{{module}}"))?;
        }
        if cx.find(THING)?.is_none() {
            cx.spawn(Spawn::new("cube").name("thing").parent(ROOT).position(Vec3::new(0.0, 0.5, 0.0)).color("#5aa9ff"))?;
        }
        Ok(())
    }

    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        let hz = u64::from(cx.tick_hz());
        let mut playing = cx.state("playing")?.as_bool().unwrap_or(false);
        let mut taps = cx.state("taps")?.as_u64().unwrap_or(0);
        let mut ticks_left = cx.state("ticks_left")?.as_u64().unwrap_or(0);
        let mut seconds = cx.state("seconds")?.as_u64().unwrap_or_else(|| seconds_for("normal"));
        let (was, mut started) = (taps, false);
        // What the engine told us since the last tick, in the order it happened: a setting the
        // rules read, a run started or given up, a press of the game's own button, the run's
        // end. In order, so that a tap that comes with the start counts, and a run started and
        // given up in one tick is given up.
        for event in cx.events().iter().filter(|e| e.from == "engine") {
            let word = |key: &str| event.data.get(key).and_then(Value::as_str);
            match event.name.as_str() {
                "setting" if word("name") == Some("difficulty") => seconds = seconds_for(word("value").unwrap_or("normal")),
                "run" if word("phase") == Some("start") => {
                    (playing, taps, ticks_left) = (true, 0, seconds * hz);
                    started = true;
                }
                "run" | "outcome" => playing = false,
                "ui" if playing && word("action") == Some("tap") => taps += 1,
                _ => {}
            }
        }
        // The player's own input (a key, a pad button, an agent's logic_input), and the clock.
        if playing {
            if cx.action("tap").pressed {
                taps += 1;
            }
            ticks_left = ticks_left.saturating_sub(1);
        }
        // What the screens show and the game file reads.
        let left = ticks_left.div_ceil(hz);
        cx.set_state("playing", playing)?;
        cx.set_state("taps", taps)?;
        cx.set_state("ticks_left", ticks_left)?;
        cx.set_state("seconds", seconds)?;
        cx.set_state("left", left)?;
        // (How many taps win is the game file's to say, in `outcome.won`: not repeated here.)
        cx.set_state("score", taps * 10 + left)?;
        // The thing rises with every tap.
        if started || taps != was {
            if let Some(thing) = cx.find(THING)? {
                cx.set(thing, Patch::new().position(Vec3::new(0.0, 0.5 + 0.3 * taps as f32, 0.0)))?;
            }
        }
        Ok(())
    }

    fn stop(&self, cx: &mut Cx, reason: StopReason) -> Result<(), SdkError> {
        // A reload keeps everything; unloading takes the module's things with it.
        if reason == StopReason::Unload {
            if let Some(root) = cx.find(ROOT)? {
                cx.delete(root)?;
            }
        }
        Ok(())
    }
}
"##;

const SMOKE: &str = r#"{
  "name": "smoke",
  "description": "Plays one run and reads how it ended: playtest_run {\"path\": \"checks/smoke.json\"}. The module must be loaded (logic_build {\"path\": \"modules/{{module}}\"}). A playtest owns the clock and steps it tick by tick, so the same spec gives the same run.",
  "steps": [
    { "at": 0, "call": "game_act", "args": { "action": "start" }, "expect": { "phase": "running" } },
    { "at": 5, "call": "game_act", "args": { "action": "tap" } },
    { "at": 10, "call": "game_act", "args": { "action": "tap" } },
    { "at": 15, "call": "game_act", "args": { "action": "tap" } },
    { "at": 20, "call": "game_act", "args": { "action": "tap" } },
    { "at": 25, "call": "game_act", "args": { "action": "tap" } }
  ],
  "ticks": 40,
  "end": [{ "call": "game_state", "expect": { "outcome": "won", "score": { "gte": 50 } } }]
}
"#;

const BRIEF: &str = r#"---
title: {{title_note}} is a new Specula project: a tap game to build on
kind: brief
about: game.json, modules/{{module}}/
---
What is here is the smallest game with every part, made by `specula new`: tap five times before
the time runs out. It has a title screen, a pause menu, settings (volume, language, difficulty),
won and lost screens, one module with the rules, and its own checks (`specula check .`).
Shapes by name (a gear, a heart, an arch, a star, a gem...) are a library that recipes and
`shape2d` items take: `specula call formae_list '{}'` names them all.
A model (a prop, a pickup, scenery) is a recipe under `assets/models/`: `notes/models.md` is
how one is written, built and looked at, in three calls.

Replace this note with what the game is to be (memory_note {id: "brief", ...}): every agent is
given it at its first call.
"#;

/// The sample game's own checks: runs, and what each must print (`specula check`).
const RUNS: &str = r#"{
  "format": "specula.checks.v1",
  "notes": "This game's own checks: `specula check .` plays each run as `specula run` would (ticks, inputs, seed) and holds its state against \"expect\" (keys of game.json's \"report\"). Checks that name the same \"settings\" share a player's settings, in this order. A check with no \"expect\" prints its state. These are the sample's, the tap game's: a game that replaces the sample deletes this file, or keeps checks of its own in it (notes/how.md).",
  "checks": [
    { "name": "the title", "ticks": 0, "expect": { "screen": "title", "phase": "title", "texts": { "has": "Start" } } },
    { "name": "five taps win", "ticks": 40, "inputs": "0:enter,5:tap,6:none,10:tap,11:none,15:tap,16:none,20:tap,21:none,25:tap", "expect": { "phase": "won", "taps": 5, "score": 60, "texts": { "has": "You won" } } },
    { "name": "the time runs out", "ticks": 320, "inputs": "0:enter,5:tap", "expect": { "phase": "lost", "taps": 1, "left": 0 } },
    { "name": "a pause holds the game", "ticks": 200, "inputs": "0:enter,5:tap,6:escape", "expect": { "phase": "paused", "left": 10 } },
    { "name": "left turns the volume down", "ticks": 4, "inputs": "0:down,1:enter,2:left,3:left", "settings": "kept", "expect": { "screen": "settings", "volume": 6 } },
    { "name": "and it is kept", "ticks": 0, "settings": "kept", "expect": { "volume": 6 } },
    { "name": "a new player has the default", "ticks": 0, "expect": { "volume": 8 } }
  ]
}
"#;

const HOW: &str = r##"---
title: How this game is built and checked
kind: how
about: game.json, modules/{{module}}/, assets/ui/
---
**Read with this page, in one go, and that is the reading done:** `game.json`,
`modules/{{module}}/src/lib.rs`, `modules/{{module}}/actions.json`, `assets/ui/hud.json`,
`assets/ui/settings.json` and `assets/strings/en.json` (the game file, the rules, the inputs,
a screen, a screen that moves settings, the words). In a new project they are a sample's, a
tap game: replace them. The other screens (`title`, `pause`, `won`, `lost`) have the same
form and are read when they are changed; `assets/ui/theme.json` is the look, and needs no
reading. Delete the sample's `checks/` folder: it checks the tap game. Every call of the SDK
that a game of this size needs is on this page.

**The loop: edit, run, read.** One command builds the modules as they are on disk, plays the
game from its start with no window, and prints how it stands as one JSON object:

    specula run . --ticks 40 --inputs "0:enter,5:tap,6:none,10:tap" [--seed 1]
                  [--settings FOLDER] [--frame out.png --size 800x480] [--follow]

- The answer is one line: the state, or an error (exit code 1). A build that fails answers
  the compiler's errors (file, line, message). A module whose call fails, or that panics,
  answers `module_failed` with the code, the message and the tick (the script's, as an
  input's is counted), never a state: an error returned from `tick` undoes all that call
  did, so the rules did not move. What the rules log (`cx.log("text")?`) is printed beside
  the state, on standard error, when the run ends. A run ends after 30 minutes at most.
- **Inputs** are words at ticks. `up down left right enter escape` press the interface, as
  keys do. A module's action (`tap` here: `modules/{{module}}/actions.json`) is held until the
  next input, as a held key is; `none` holds nothing. A word that is both does both. Words
  joined by `+` are held together (`up+shoot`). An action that is not a button takes its
  value (`throttle=0.5`, `move=1/0`). In the rules read `cx.action("tap").pressed` for
  "once" and `.held` for "while held".
- **Ticks.** The script counts every tick. The game's own clock ticks only while a run is
  under way: the title, a pause and an ended run hold it, as they do for a player.
- **What is printed** is the game file's `"report"`: each line a name and the key whose value
  it shows. Keys: `{{module}}.KEY` (what the module keeps), `setting.NAME`, `game.phase`
  (title, running, paused, won, lost), `game.score`, `game.outcome`, `game.tick`, `run.ticks`,
  `run.seed`, `ui.screen` (the top screen's name), `ui.focus` and `ui.focus_index` (the
  focused button, and its place among its screen's buttons), `ui.texts` (every text on
  screen, in order) and `ui.top_texts` (the top screen's). No `"report"`: all of it.
- `--seed N` (1 when left out) seeds `cx.random_*`: the same seed and inputs give the same
  run, and the rules never see a seed. `--settings FOLDER` is where the player's settings are
  read and kept (a file the engine writes in it: the same folder again finds them); without
  it they are kept nowhere. A window's player has a folder of their own, per user.
- `--frame` writes what a player would see (the game file's `"view"`, with the interface).
- `--follow` goes on after the script (`--ticks` may be left out): the state is printed,
  then each line read from standard input is a step, `TICKS` or `TICKS WORD` (`10 up+shoot`:
  that input, then ten ticks), and the state is printed after it, until the input ends. It
  is how a program of yours plays by reaction, in one run, should a script of inputs not do.

**Shapes by name:** `specula call formae_list '{}'` names a library of figures and solids (a
gear, a heart, an arch, a star, a torus, a gem...) that model recipes and `shape2d` items take
by name; `formae_get` gives one ready to paste.

**Models** (props, pickups, scenery) are recipes under `assets/models/`: `notes/models.md`.

**game.json** (an unknown key is refused, by name)

- `title`; `tick_hz` (1 to 240; 30 when left out); `modules` (folders); `locale`; `start`:
  `"title"` (the title screen first) or `"run"` (straight into a run).
- `screens`: which file under `assets/ui` is the `title`, `pause`, `won`, `lost` and `hud`.
- `settings`: each `{"name", "kind", "default"}`, the kind one of `"int"` (with `"min"`,
  `"max"`), `"choice"` (with `"options"`), `"toggle"`, `"locale"`. `language` and `volume`
  are there unless the game declares its own: `volume` is an int from 0 to 10, 8 at first,
  and it is the sound's volume. A setting of the game's with `"is": "volume"` (any name, its
  own range and default) is the volume in its place.
  Every setting is kept for the player; nothing else is needed for that.
- **A setting the rules read** (a difficulty) is four things: `"sim": true` on it here; a
  button on a screen with the action `cycle:NAME` (or `step:NAME:+1`), its text
  `{setting.NAME.text}`; for a choice, a line `setting.NAME.VALUE` for each value in every
  language's strings; and in the rules the event `setting` (`name`, `value`), which the
  engine sends at the start of every run, and when it is changed during one.
- `outcome`: `"won"` and `"lost"` are conditions over keys: `{"key": "{{module}}.taps",
  "gte": 5}` (also `eq ne lt lte gt in`), `{"all": [..]}`, `{"any": [..]}`, `{"not": ..}`.
  `"score"` names the key that holds the score.
- `view`: how the game is looked at, by the window and by a frame. A 2D game:
  `{"center": [x, y], "height": metres}` (straight on, +x right, +y up: draw with flat cubes
  at z 0). 3D: `{"eye": [x, y, z], "target": [x, y, z], "fov_y_deg": 50}`.
- `report`: above.

**Screens** (`assets/ui/*.json`; widgets: box, panel, label, button, bar, list, spacer)

- A text is words, or `@key` (a line of `assets/strings/<locale>.json`). In either, `{KEY}`
  shows a value: `{{{module}}.taps}`, `{setting.volume}`, `{game.score}`, and a setting's
  value as words, `{setting.NAME.text}`: a language's own name, and for a choice the line
  `setting.NAME.VALUE` of the strings (`"setting.difficulty.easy": "Easy"`), in the player's
  language. `"visible"` shows a widget by a condition: `"KEY"`, `"!KEY"`, or a comparison
  (`"setting.difficulty == 'hard'"`, `"{{module}}.left < 3"`; `&&`, `||`).
- A button's `action` is a word. The engine does `start`, `pause`, `resume`, `title`,
  `open:SCREEN`, `back`, `quit`, `set:NAME:VALUE`, `step:NAME:+1`, `step:NAME:-1`,
  `cycle:NAME`, `save:SLOT`, `load:SLOT`; any other word reaches the rules as the event `ui`.
- On a button that moves a setting, Left and Right step it down and up; Enter does its action.
- Another language is another file in `assets/strings`: the language setting finds it. The
  game's name on the title screen (`{game.title}`) is the game file's `title`, or the line
  `title.name` of the strings when they have one (in every language's file, as any line).

**The rules** (`modules/{{module}}/src/lib.rs`: `tick` runs once a tick; crate `specula-sdk`)

- Keep: `cx.state("k")?` (a `Value`; then `.as_u64()`, `.as_i64()`, `.as_f64()`,
  `.as_bool()`, `.as_str()`, `.as_list()` (a `&[Value]`), `.as_map()`, `.get("key")`, each an
  `Option`) and `cx.set_state("k", value)?`. A value is a number, a bool,
  text, a `Vec<Value>` or a map (`Value::map().with("x", 1.5).with("kind", "walker")`); `bool`,
  `i32`, `i64`, `u32`, `u64`, `usize`, `f32`, `f64`, `&str` and `String` turn into one.
  Reading asks the engine, as writing does: a helper of yours that reads takes `cx: &mut Cx`. Keep
  everything there and nothing in the struct: what is kept is what screens show, what
  `outcome` reads and what `report` prints.
- Input: `cx.action("name")` has `.pressed`, `.held`, `.released` and `.value` (`[f32; 2]`).
  The actions are `modules/{{module}}/actions.json`: each `{"name", "kind": "button" (or
  `"axis"`, `"axis2"`), "group", "description", "bindings": ["Key.Space", "Pad.South"]}`.
  Sources, the ones most games bind: `Key.A` to `Key.Z`, `Key.0` to `Key.9`, `Key.Up`,
  `Key.Down`, `Key.Left`, `Key.Right`, `Key.Space`, `Key.Enter`, `Key.Escape`, `Key.Tab`,
  `Key.Shift`, `Key.Ctrl`; `Pad.South`, `Pad.East`, `Pad.West`, `Pad.North`, `Pad.DpadUp`
  (`Down`, `Left`, `Right`), `Pad.Start`, `Pad.LeftBumper`, `Pad.RightTrigger` (an axis),
  `Pad.LeftStick` (an axis2); `Mouse.Left`, `Mouse.Right`; `Touch.Tap`. A name the input
  map does not read is refused when the module loads, with the name.
- Events from the engine, at a tick: `for e in cx.events()`, with `e.from == "engine"`,
  `e.name` and `e.data.get("key")`: `run` (`phase` is `start` when a run begins; any other
  `run`, and `outcome`, end it), `setting` (`name`, `value`), `ui` (`action`).
- The world: `cx.spawn(Spawn::new("cube").name("n").parent("/{{module}}").position(v)
  .scale(v).color("#rrggbb").tags(&["mob"]))?` (an `Id`; `v` is `Vec3::new(x, y, z)`, `f32`),
  `cx.find("/{{module}}/n")?` (an `Option<Id>`), `cx.set(id, Patch::new().position(v)
  .field("color", "#rrggbb"))?`, `cx.delete(id)?`, `cx.query(Some("mob"), None, 4096)?` (a
  `Vec<Id>`).
- Things that come and go (shots, enemies): keep them as a list in the state, and each tick
  delete what `cx.query(Some("mob"), None, 4096)?` gives and spawn them again with that tag
  and **no name** (the engine names them: two children of one parent with one name is the
  error `name_taken`). A query under a path (`Some("/{{module}}/mobs")`) gives the entity at
  that path too, with what is below it.
- `cx.tick()` (`u64`), `cx.tick_hz()` (`u32`), `cx.dt()` (the seconds of a tick, `f32`),
  `cx.entity(id)?` (an `Option`, with `.position()`), `cx.random_below("stream", n)?` (a `u32`
  below `n`), `cx.random_f32("stream")?` (0 to 1), `cx.sound("name", None, 1.0)?`,
  `cx.log("text")?`. The random calls are seeded by the run (`--seed`, a check's `"seed"`):
  the rules take no seed and keep none. A stream goes on from where it was, so a second game
  in one session is another game; `cx.random_restart("stream")?` puts it back at its
  beginning (call it where a run begins, for "the same seed gives the same game again").

**Also**

- `specula play . --as-is` opens the window and plays what is on disk.
- `specula check .` plays every run that `checks/runs.json` lists and says which did not hold:
  checks kept with a game, for one that someone comes back to or that is handed on
  (`notes/checks.md` says how they are written). A game that came with checks of its own: run
  them after a change. Building a game does not need them.
- An engine left running serves the project's tools: `specula open . -- --headless --lead
  NAME`, then `specula tools` lists what an agent can call there (`logic_build`,
  `game_state`, `game_act`, `playtest_run`, `ui_frame`). A playtest is steps and what to
  expect, as a file (the sample's was `checks/smoke.json`).
- `notes/` is what the project remembers. Leave a note of what you decided and why:
  `notes/NAME.md`, with the front matter this file has (`title`, `kind: decision`, `about`).
"##;

/// How a model is made through the engine, in one page: what a measured comparison's cold
/// builders had to find out by refused builds, a 62 KB design document and scripts of their own
/// (2026-11-21). Every call it names is held by a test below.
const MODELS_NOTE: &str = r##"---
title: How a model is written, built and looked at (recipes)
kind: fact
about: assets/models/, assets/style.json
---
A model (a prop, a pickup, scenery) is a recipe: one JSON file under `assets/models/`, which
the engine builds. Three calls, with an engine running on this project:

    specula call model_schema '{}'

says how a recipe is written, whole, with one that builds to start from (`{"of": "extrude"}`:
one shape, field by field; `{"of": "sdf"}`: a sculpt, with a soft heart to start from). Ask it
before writing: a field that is guessed wrong is a refused build.

    specula call model_build '{"recipe": "models/NAME.json"}'

builds it and answers its triangles, its bounds, `closed` (true: watertight) and `open_edges`.
Nothing needs exporting, and no script of your own, to learn those. A recipe that is wrong is
refused with the field and what is accepted there. `specula call model_build_all '{}'` builds
every recipe in `assets/models/` and answers a line each (triangles, size, closed, or why it
was refused).

    specula call studio_sheet '{"recipe": "models/NAME.json"}'

pictures it from four sides in one image (`image.path` in the answer): look at it. For one
large front view: `"views": ["front"], "tile": 512`. For several models in one image:
`"subjects": ["models/a.json", "models/b.json"]`. To see the form alone: `"passes": ["colour",
"clay"]`.

- **Sizes** are metres, +Y up. A mesh shape (`box`, `cylinder`, `sphere`...) stands on y = 0,
  centred on the Y axis. A part's `ops` move and turn it, in order:
  `{"op": "rotate", "deg": [90, 0, 0]}`, `{"op": "move", "by": [0, 0.15, 0]}`.
- **Materials** are the names in `assets/style.json` (`matte`, `painted`, `metal`, `glow` to
  start with), and a part's `color` is a palette name there or `"#rrggbb"`. Change the style
  and every model changes with it.
- **Shapes by name**: `specula call formae_list '{}'` names the library and `formae_get
  {"name": "gear"}` gives the snippet to paste. A named plane figure lies flat in the ground
  plane: raise it (`"raise": {"as": "slab", "thickness": 0.05}`) and stand it up with the
  rotate above. A round solid by name is thousands of triangles unless told: `"detail": 16`
  for a small prop.
- **A soft edge** on a raised figure: `"raise": {"as": "bevelled", "thickness": T, "width":
  W}` or `{"as": "pillow", "thickness": T, "rise": R, "inset": I}`. A figure that narrows to a
  point or has a sharp notch (a heart, a crescent) takes almost no inset (the refusal says how
  deep it can go): sculpt those (`model_schema '{"of": "sdf"}'`).
- **A budget**: `"budget": {"max_triangles": N}` in a recipe refuses a build over it, with the
  count.
"##;

const CHECKS_NOTE: &str = r##"---
title: How checks kept with a game are written (specula check)
kind: fact
about: checks/runs.json
---
`checks/runs.json` lists runs of the game and what the state of each must be: for a game that
someone comes back to, or that is handed on. Building a game does not need them (`specula run`
is the loop: notes/how.md).

    specula check . [--only NAME]

    { "name": "five taps win", "ticks": 40, "inputs": "0:enter,5:tap,6:none,10:tap",
      "seed": 1, "settings": "kept", "frame": "checks/out/won.png", "size": [800, 480],
      "expect": { "phase": "won", "taps": 5, "score": { "gte": 50 },
                  "texts": { "has": "You won" } } }

- Each check is a run as `specula run` takes one, with what its state must be. The command
  plays them all and prints a line a check (`ok`, or what `differs` and the state) and a line
  that counts them; exit code 1 when one did not hold. A check with no `"expect"` prints its
  state. A game that came with checks of its own: run them after a change.
- `"expect"`: a key of the report (or a path into it: `walkers.0.health`) with its value, or
  with comparisons that must all hold: `{"ne": v}`, `{"lt": n}`, `{"lte": n}`, `{"gt": n}`,
  `{"gte": n}`, `{"has": v}` (a list holds it; a text holds it), `{"len": n}`; a range is
  `{"gt": 8.4, "lt": 8.6}`. A value (and `ne`) is equal within a rounding: expect `8.5` of
  8.500000000000009. `lt lte gt gte` take the number as it is, so a range leaves a margin.
  An object is comparisons only, or a value to equal with none of those words in it.
  A key the state does not have never holds, whatever is expected of it.
- Checks that name the same `"settings"` (small letters) share one player's settings, in the
  file's order: set in one check and found in the next is "kept between runs", shown. A check
  that names none has a new player's. `--only NAME` plays that check alone, with nothing an
  earlier check set.
- A `"frame"` is a PNG below the project's `checks` folder.
"##;

const GITIGNORE: &str = "# Build output and what the engine keeps beside a project.\n/target/\n/.specula-cache/\n/.specula-trace/\n/captures/\n";

/// The files of a new project named `name`, each by its path below the project (with `/`) and
/// its text: everything but the marker and the SDK. `module`: its module's name.
pub fn files(name: &str, module: &str) -> Vec<(String, String)> {
    // (The role's word is the authority table's own: no source but that one spells it.)
    let fill = |text: &str| text.replace("{{lead}}", crate::authority::ROLE_LEAD).replace("{{module}}", module).replace("{{title}}", &quoted(name)).replace("{{title_toml}}", &quoted(name)).replace("{{title_note}}", &name.replace(['\n', '\r'], " "));
    let mut out: Vec<(String, String)> = Vec::new();
    out.push(("game.json".to_owned(), fill(GAME)));
    // The shell's own screens and words, with this game's HUD and its three lines.
    for (path, text) in crate::shell::BUILTIN {
        let text = match *path {
            "assets/ui/hud.json" => fill(HUD),
            // (After the score's line, whatever the file's line ends are in this checkout.)
            "assets/strings/en.json" => text
                .replace(
                    "\"hud.score\": \"Score: {game.score}\",",
                    &fill("\"hud.score\": \"Score: {game.score}\",\n    \"hud.taps\": \"Taps: {{{module}}.taps}\",\n    \"hud.left\": \"Time: {{{module}}.left}\",\n    \"hud.tap\": \"Tap\","),
                )
                // The game's own setting, and its values as words (`setting.NAME.VALUE`).
                .replace(
                    "\"settings.back\": \"Back\",",
                    "\"settings.difficulty\": \"Difficulty: {setting.difficulty.text}\",\n    \"setting.difficulty.easy\": \"Easy\",\n    \"setting.difficulty.normal\": \"Normal\",\n    \"setting.difficulty.hard\": \"Hard\",\n    \"settings.back\": \"Back\",",
                ),
            // (Its button, before Back.)
            "assets/ui/settings.json" => text.replace(
                "{ \"type\": \"button\", \"id\": \"settings_back\",",
                "{ \"type\": \"button\", \"id\": \"difficulty\", \"text\": \"@settings.difficulty\", \"action\": \"cycle:difficulty\" },\n          { \"type\": \"button\", \"id\": \"settings_back\",",
            ),
            _ => (*text).to_owned(),
        };
        out.push(((*path).to_owned(), text));
    }
    out.push(("assets/style.json".to_owned(), STYLE.to_owned()));
    out.push(("Cargo.toml".to_owned(), WORKSPACE.to_owned()));
    out.push((format!("modules/{module}/Cargo.toml"), fill(MODULE_MANIFEST)));
    out.push((format!("modules/{module}/actions.json"), fill(MODULE_ACTIONS)));
    out.push((format!("modules/{module}/src/lib.rs"), fill(MODULE_SOURCE)));
    out.push(("checks/smoke.json".to_owned(), fill(SMOKE)));
    out.push((crate::run_checks::FILE.to_owned(), RUNS.to_owned()));
    out.push(("notes/brief.md".to_owned(), fill(BRIEF)));
    out.push(("notes/how.md".to_owned(), fill(HOW)));
    out.push(("notes/checks.md".to_owned(), fill(CHECKS_NOTE)));
    out.push(("notes/models.md".to_owned(), MODELS_NOTE.to_owned()));
    out.push((".gitignore".to_owned(), GITIGNORE.to_owned()));
    out
}

/// What [`write`] made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Made {
    pub module: String,
    /// Files written, the SDK's included.
    pub files: usize,
}

/// Writes a new project's files into `folder` (made by `launch_store::create`: it holds its
/// marker and an empty `assets`), then this engine's SDK. Nothing that is there is replaced.
pub fn write(folder: &Path, name: &str) -> Result<Made, String> {
    let module = module_name(name);
    let book = crate::disk::book();
    let mut written = 0;
    for (path, text) in files(name, &module) {
        let at = path.split('/').fold(folder.to_path_buf(), |p, part| p.join(part));
        if at.exists() {
            continue;
        }
        book.write(crate::disk::Category::Project, "new", &at, text.as_bytes(), crate::disk::Need::Essential).map_err(|e| format!("{path} could not be written ({e})"))?;
        written += 1;
    }
    let sdk = specula_logic::sdk_source::write_into(folder).map_err(|e| format!("the SDK could not be written ({e})"))?;
    Ok(Made { module, files: written + sdk.len() })
}

/// Makes a new project that builds and plays: the folder `parent/name` with its marker
/// (`launch_store::create`: the folder must not exist, or be empty), then everything [`write`]
/// writes. Returns the project's folder and what was made.
///
/// `records`: where the person who made it keeps which module sources may run at start
/// ([`crate::approved`]). The module written here is the engine's own text, so it is recorded
/// as built: a new project plays (`specula play`) before anyone has built it by hand.
pub fn create(parent: &Path, name: &str, records: Option<&Path>) -> Result<(std::path::PathBuf, Made), String> {
    let folder = crate::launch_store::create(parent, name)?;
    let made = write(&folder, name).map_err(|why| format!("{} was made, and not all of it: {why}", folder.display()))?;
    if let Some(records) = records {
        let module = folder.join("modules").join(&made.module);
        // (Both are the engine's own text: the module's folder, and the project root's build
        // configuration, which is the workspace manifest it just wrote and nothing else.)
        if let (Ok(digest), Ok(root)) = (specula_logic::digest::module_digest(&module), specula_logic::digest::root_config_digest(&folder)) {
            // (Not recorded is not fatal: the project is whole, and its owner builds the module.)
            let _ = crate::approved::record_in(records, &folder, &module, &digest.hex, &root.hex);
        }
    }
    Ok((folder, made))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_is_named_from_its_project() {
        assert_eq!(module_name("crypt"), "crypt");
        assert_eq!(module_name("Hold the Gate!"), "hold_the_gate");
        assert_eq!(module_name("2 Fast"), "fast");
        assert_eq!(module_name("Frozen  Wasteland 2"), "frozen_wasteland_2");
        assert_eq!(module_name("game"), "play", "a screen's own keys are game.*");
        assert_eq!(module_name("UI"), "play");
        assert_eq!(module_name("specula-sdk"), "play", "never the SDK's own name");
        assert_eq!(module_name("\u{6c37}\u{306e}\u{57ce}"), "play", "no letter a crate's name takes");
        assert!(module_name(&"long name ".repeat(10)).len() <= 32);
    }

    /// Every file of a new project is what its reader takes: the game file parses and names the
    /// module and keys the module keeps, the screens and words load as a whole interface, the
    /// style is one, the notes are notes with a brief, the check is JSON, and nothing still
    /// holds a placeholder.
    #[test]
    fn a_new_projects_files_are_each_what_their_reader_takes() {
        let name = "Hold the Gate, it's ours";
        let module = module_name(name);
        assert_eq!(module, "hold_the_gate_it_s_ours");
        let made = files(name, &module);
        let text = |path: &str| made.iter().find(|(p, _)| p == path).map(|(_, t)| t.as_str()).unwrap_or_else(|| panic!("no {path}"));
        for (path, text) in &made {
            assert!(!text.contains("{{"), "{path} holds a placeholder");
        }
        let game = specula_ludus::GameFile::parse(text("game.json")).expect("the game file");
        assert_eq!((game.title.as_str(), game.modules.as_slice(), game.tick_hz), (name, [format!("modules/{module}")].as_slice(), 30));
        let mut reads = game.outcome.keys();
        reads.sort_unstable();
        assert_eq!(reads, [format!("{module}.left"), format!("{module}.score"), format!("{module}.taps")].iter().map(String::as_str).collect::<Vec<_>>());
        // The interface: the shell made from these very files has every screen and no problem.
        let mut shell_files = crate::shell::ShellFiles { game: text("game.json").to_owned(), theme: text("assets/ui/theme.json").to_owned(), ..Default::default() };
        for (path, text) in &made {
            let stem = || path.rsplit('/').next().unwrap().trim_end_matches(".json").to_owned();
            if path.starts_with("assets/ui/") && !path.ends_with("theme.json") {
                shell_files.screens.push((stem(), text.clone()));
            } else if path.starts_with("assets/strings/") {
                shell_files.tables.push((stem(), text.clone()));
            }
        }
        let shell = crate::shell::Shell::build(&module, shell_files).expect("a shell");
        let state = crate::game::Game::game_state(&shell).unwrap();
        assert_eq!(state.get("problems"), None, "{state}");
        assert_eq!(state.get("phase").and_then(Json::as_str), Some("title"));
        // Every word a screen shows is in the table.
        let table = specula_lingua::Table::parse(text("assets/strings/en.json")).unwrap();
        for (path, screen) in made.iter().filter(|(p, _)| p.starts_with("assets/ui/")) {
            for (at, _) in screen.match_indices("\"@") {
                let key: String = screen[at + 2..].chars().take_while(|c| *c != '"').collect();
                assert!(table.entries.contains_key(&key), "{path} shows @{key}, which the words do not have");
            }
        }
        // The style, the module's own files, the check and the notes.
        specula_model::Style::parse(text("assets/style.json")).expect("the style");
        let manifest = text(&format!("modules/{module}/Cargo.toml"));
        assert!(crate::project_kinds::check_module_manifest("Cargo.toml", manifest).is_empty(), "the module's manifest is a module's");
        assert!(manifest.contains(&format!("name = \"{module}\"")) && manifest.contains("The rules of Hold the Gate, it's ours, as"));
        specula_core::toml::parse(manifest).expect("the module's manifest is TOML");
        // A name with a quote or a backslash in it stays one string, in JSON and in TOML.
        assert_eq!(quoted("a \"b\" \\ c"), "a \\\"b\\\" \\\\ c");
        Json::parse(text(&format!("modules/{module}/actions.json"))).expect("the actions");
        let source = text(&format!("modules/{module}/src/lib.rs"));
        assert!(source.contains(&format!("name: \"{module}\"")) && source.contains(&format!("const ROOT: &str = \"/{module}\";")));
        let smoke = Json::parse(text("checks/smoke.json")).expect("the check");
        assert_eq!(smoke.get("steps").and_then(Json::as_array).map(<[Json]>::len), Some(6));
        let (notes, bad) = specula_memoria::Notes::read([("brief", text("notes/brief.md")), ("how", text("notes/how.md")), ("checks", text("notes/checks.md")), ("models", text("notes/models.md"))]);
        assert!(bad.is_empty(), "{bad:?}");
        // The models' page (2026-11-21): one page, pointed at from the brief and the guide, and
        // every tool, shape, operation, raise and material it names is one the engine has.
        let models = text("notes/models.md");
        assert!(models.len() <= 3 * 1024, "notes/models.md is {} bytes: it is one page", models.len());
        assert!(notes.brief().contains("notes/models.md") && text("notes/how.md").contains("notes/models.md"), "the models' page is not pointed at");
        let tools = crate::tools::definitions();
        for tool in ["model_schema", "model_build", "model_build_all", "studio_sheet", "formae_list", "formae_get"] {
            assert!(models.contains(tool) && tools.iter().any(|d| d.name == tool), "notes/models.md and the engine's tools differ on {tool}");
        }
        for of in ["extrude", "sdf", "box", "cylinder", "sphere", "rotate", "move"] {
            assert!(models.contains(of) && specula_model::recipe_schema(Some(of)).is_ok(), "notes/models.md names {of}, which a recipe does not take");
        }
        let style = specula_model::Style::parse(text("assets/style.json")).expect("the style");
        for material in ["matte", "painted", "metal", "glow"] {
            assert!(models.contains(&format!("`{material}`")) && style.materials.contains_key(material), "notes/models.md names the material {material}, which the project's style has not");
        }
        // The answer it promises is the answer a build gives, and its example builds here.
        let example = specula_model::build(specula_model::SCHEMA_EXAMPLE, &specula_model::BuildParams::new(&style)).expect("model_schema's example builds under a new project's style");
        let report = example.report();
        assert!(report.get("closed").is_some() && report.get("open_edges").is_some() && models.contains("`closed`") && models.contains("`open_edges`"), "{report:?}");
        let brief = notes.brief();
        assert!(brief.contains("is a new Specula project") && brief.contains("How it is built and checked") && brief.contains("specula run . --ticks"), "{brief}");
        // The shape library is pointed at where a cold agent already looks (stage 3b of the
        // library): the brief given at the first call, and the guide.
        assert!(brief.contains("specula call formae_list") && text("notes/how.md").contains("specula call formae_list"), "{brief}");
        // The guide names every key a game file takes (what the comparison's builders opened
        // the engine's design documents for), and the sample's game file shows a report.
        let how = text("notes/how.md");
        for key in ["title", "tick_hz", "modules", "locale", "start", "screens", "settings", "outcome", "view", "report"] {
            assert!(how.contains(&format!("`{key}`")), "notes/how.md does not name the game file's {key}");
        }
        // Every source the guide names for a binding is one the input map reads (a builder of
        // the second pass searched the engine's source three times for how keys are spelt).
        let mut sources = 0;
        for word in how.split('`').filter(|w| ["Key.", "Pad.", "Mouse.", "Touch."].iter().any(|d| w.starts_with(d))) {
            assert!(specula_input::Source::parse(word).is_some(), "notes/how.md names the source {word}, which the input map does not read");
            sources += 1;
        }
        assert!(sources >= 20, "{sources} sources named");
        for said in ["module_failed", "name_taken", "--follow", "TICKS WORD", "no name", "specula check .", "checks/runs.json", "random_restart", "from 0 to 10", "assets/ui/settings.json"] {
            assert!(how.contains(said), "notes/how.md does not say {said}");
        }
        // The checks' own note says how one is written (it was a section of the guide until the
        // sixth pass: every build read it first and wrote checks it was not asked for).
        let checks_note = text("notes/checks.md");
        for said in ["specula check . [--only NAME]", "\"expect\"", "\"settings\"", "{\"gt\": 8.4, \"lt\": 8.6}"] {
            assert!(checks_note.contains(said), "notes/checks.md does not say {said}");
        }
        assert!(!how.contains("\"expect\""), "the guide describes checks again: that is notes/checks.md's");
        // The sample's own checks are read as checks, and each expects keys the sample's game
        // file reports (what they come to when played is the live check's to hold).
        let runs = crate::run_checks::parse(text(crate::run_checks::FILE)).expect("the sample's checks");
        let reported = specula_ludus::GameFile::parse(text("game.json")).expect("the game file").report;
        assert!(runs.len() >= 5, "{} checks", runs.len());
        for check in &runs {
            for (key, _) in &check.expect {
                assert!(reported.iter().any(|(name, _)| name == key), "the sample's check '{}' expects '{key}', which its game file does not report", check.name);
            }
        }
        // The game's own setting has its button, and its values their words.
        assert!(text("assets/ui/settings.json").contains("\"action\": \"cycle:difficulty\""), "the sample's settings screen has no button for its difficulty");
        let words = specula_lingua::Table::parse(text("assets/strings/en.json")).expect("the words");
        for line in ["settings.difficulty", "setting.difficulty.easy", "setting.difficulty.normal", "setting.difficulty.hard"] {
            assert!(words.entries.contains_key(line), "the sample's words have no {line}");
        }
        let game = specula_ludus::GameFile::parse(text("game.json")).expect("the game file");
        assert!(game.report.iter().any(|(name, key)| name == "taps" && *key == format!("{module}.taps")), "{:?}", game.report);
        assert!(brief.chars().count() <= specula_memoria::MAX_BRIEF);
        // The workspace takes every module folder and the SDK the engine writes.
        let workspace = specula_core::toml::parse(text("Cargo.toml")).expect("the workspace");
        assert_eq!(workspace.get("workspace").and_then(|w| w.get("members")).and_then(Json::as_array).map(<[Json]>::len), Some(3));
    }

    /// The project as written builds with Cargo and nothing else: its module against the SDK
    /// the engine wrote, in its own workspace. Skipped, and said so, where there is no cargo.
    #[test]
    fn the_written_project_builds_on_its_own() {
        let parent = std::env::temp_dir().join(format!("specula-scaffold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        let folder = crate::launch_store::create(&parent, "Tap Five").expect("a project folder");
        let made = write(&folder, "Tap Five").expect("the project's files");
        assert_eq!(made.module, "tap_five");
        assert!(made.files > 20, "{made:?}");
        for path in ["game.json", "assets/ui/hud.json", "assets/strings/en.json", "assets/style.json", "modules/tap_five/src/lib.rs", "crates/specula-sdk/Cargo.toml", "notes/how.md", "checks/smoke.json"] {
            assert!(path.split('/').fold(folder.clone(), |p, part| p.join(part)).is_file(), "{path} was not written");
        }
        assert!(specula_logic::sdk_source::differs(&folder).is_empty(), "the SDK is this engine's");
        // A second writing replaces nothing a person may have changed.
        std::fs::write(folder.join("game.json"), "{}").unwrap();
        write(&folder, "Tap Five").unwrap();
        assert_eq!(std::fs::read_to_string(folder.join("game.json")).unwrap(), "{}");
        let cargo = std::process::Command::new("cargo").args(["check", "--offline", "--workspace"]).current_dir(&folder).env_remove("RUSTFLAGS").env("CARGO_TARGET_DIR", parent.join("t")).output();
        match cargo {
            Err(e) => crate::skip_test("the_written_project_builds_on_its_own", &format!("no cargo to build with ({e})")),
            Ok(out) => assert!(out.status.success(), "the project as written does not build:\n{}", String::from_utf8_lossy(&out.stderr)),
        }
        let _ = std::fs::remove_dir_all(parent);
    }
}

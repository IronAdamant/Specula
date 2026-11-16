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
//! | `checks/smoke.json` | a playtest that plays a run and reads how it ended |
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
    const TAKEN: [&str; 16] = ["game", "setting", "ui", "item", "engine", "world", "test", "core", "std", "self", "super", "crate", "mod", "use", "fn", "specula"];
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

const MODULE_SOURCE: &str = r##"//! The rules of the game, as a Specula module (docs: notes/how.md in this project).
//!
//! This is the smallest game with every part: tap five times before the time runs out. Replace
//! the rules and keep the shape:
//!
//! - **Stateless code.** What the game must remember lives in the state store (`cx.set_state`)
//!   and in the world, never in this struct: a rebuild reloads while the game runs and loses
//!   nothing.
//! - **What a screen shows is what you keep.** The HUD's `{{{module}}.taps}` is the value this
//!   module set with `cx.set_state("taps", ..)`; the game file (game.json) decides a run is won
//!   or lost from the same keys. There is no call to draw an interface.
//! - **The tick is the only clock**, and randomness comes from `cx.random_*`: the same inputs
//!   at the same ticks give the same game (a playtest replays it).
//! - **What happens outside the rules arrives as events** from the engine, at a tick: `run`
//!   (a run starts, or was given up), `setting` (a setting the rules read), `ui` (a button of
//!   the game's own was pressed), `outcome` (the run ended).

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
won and lost screens, one module with the rules, and one check that plays a run.

Replace this note with what the game is to be (memory_note {id: "brief", ...}): every agent is
given it at its first call.
"#;

const HOW: &str = r#"---
title: How this game is built and checked
kind: how
about: game.json, modules/{{module}}/, assets/ui/, checks/
---
**The loop** (one engine left running; from a shell a tool is `specula call TOOL '{json}'`)

1. Edit `modules/{{module}}/src/lib.rs`, then `logic_build {"path": "modules/{{module}}"}`: it
   builds, reloads into the running game, and answers the compiler's diagnostics as data (it
   needs the owner or a lead the operator granted: `--lead NAME`, then `agent_hello {name,
   role: "{{lead}}"}`).
2. `game_act {"action": "start"}` starts a run; `game_state` says how it stands and why (each
   key the outcome reads, with its value now).
3. `time_control {"action": "step", "ticks": 30}` runs a paused clock tick by tick;
   `logic_input` presses the module's actions as a player would.
4. `playtest_run {"path": "checks/smoke.json"}` plays a whole run and checks how it ended.
5. `ui_frame` shows the interface as a picture; `ui_check` finds text that does not fit.

**Where things live**

- `game.json`: the title, the tick rate, the modules, the settings, and when a run is won or
  lost (`outcome`, over the keys the module keeps). The engine follows it while it runs.
- `modules/{{module}}/src/lib.rs`: the rules. They keep what matters with `cx.set_state`; that
  is what screens show (`{{{module}}.taps}`) and what `outcome` reads. No code draws menus.
- `assets/ui/*.json`: the screens (title, pause, settings, won, lost, hud). A button's `action`
  is a word: the engine does `start`, `pause`, `resume`, `title`, `open:NAME`, `back`,
  `set:NAME:VALUE`, `step:NAME:+1`, `cycle:NAME`, `save:SLOT`, `load:SLOT`; any other word
  reaches the module as the event `ui` at the next tick.
- `assets/strings/en.json`: the words (`@key` in a screen). Another language is another file.
- `assets/style.json`: the palette and materials model recipes name.
- `checks/`: playtests as data. Add one for each thing the game must do.
- `notes/`: what the project remembers (`memory_note`, `memory_list`, `memory_why`).

**Playing it**: `specula play NAME` opens the window in front and loads the module as its owner
or lead last built it (`logic_build`). Sources changed since are not loaded at start until one of
them builds again: `game_state` says so under `problems`. `specula play NAME --as-is` runs what
is on disk now, whoever wrote it. `specula open NAME` shows the screens and runs no code.
"#;

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
            "assets/strings/en.json" => text.replace(
                "\"hud.score\": \"Score: {game.score}\",",
                &fill("\"hud.score\": \"Score: {game.score}\",\n    \"hud.taps\": \"Taps: {{{module}}.taps}\",\n    \"hud.left\": \"Time: {{{module}}.left}\",\n    \"hud.tap\": \"Tap\","),
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
    out.push(("notes/brief.md".to_owned(), fill(BRIEF)));
    out.push(("notes/how.md".to_owned(), fill(HOW)));
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
        let (notes, bad) = specula_memoria::Notes::read([("brief", text("notes/brief.md")), ("how", text("notes/how.md"))]);
        assert!(bad.is_empty(), "{bad:?}");
        let brief = notes.brief();
        assert!(brief.contains("is a new Specula project") && brief.contains("How it is built and checked") && brief.contains("logic_build"), "{brief}");
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

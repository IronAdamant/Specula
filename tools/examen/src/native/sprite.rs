//! `sprite_check`: `sprite_bake` end to end through a running engine, as an agent with a shell
//! uses it (docs/design/SPRITE_BAKE.md). A project made by `specula new` is given a copy of
//! Sepulcra's recipes, animations and style files, and then:
//!
//! - a shambler's walk is baked: the atlas and its frame data are two project files written in
//!   **one** journal entry, and the frame data reads as the 2D path's own sprite objects;
//! - `world_undo` takes both back at once;
//! - a bake lit by the project's light says so (`style.toml`'s `[stage]` game rig);
//! - a long bake of one agent's is cancelled only by that agent: another agent's cancel is
//!   refused and names the owner, the owner's cancel from a second connection stops it, and
//!   nothing of it is written;
//! - a bake of the most cells a call takes (512) answers with its sheet, and the log says what
//!   it cost (the engine's own `cost` record).

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use specula_core::Json;

use super::{at, expect, text, Cx};

/// What of Sepulcra's assets the project gets (read from the checkout, never changed).
const COPIED: [&str; 4] = ["style.json", "style.toml", "recipes", "anims"];

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_dir() {
        std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ()).map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
    }
}

/// The paths `project_list` has under `assets/sprites`.
fn sprites(cx: &mut Cx, port: u64, who: Option<(&str, &str)>) -> Result<Vec<String>, String> {
    let listed = cx.call(port, who, "project_list", r#"{"prefix": "assets/sprites"}"#)?;
    Ok(at(&listed, "files").and_then(Json::as_array).map(|f| f.iter().filter_map(|f| text(f, "path").map(str::to_owned)).collect()).unwrap_or_default())
}

pub fn sprite(cx: &mut Cx) -> Result<String, String> {
    let (ann, bob) = (Some(("ann", "builder")), Some(("bob", "builder")));
    let settings = cx.settings();
    let projects = cx.out.join("p");
    let project = projects.join("bake");
    if project.exists() {
        return Err(format!("{} is there already: give the check an output folder of its own (it never deletes one)", project.display()));
    }
    let (code, made) = cx.cli(&["new", "bake", "--in", &projects.to_string_lossy(), "--settings", &settings])?;
    expect(code == 0, "`specula new` did not make the project", &made)?;
    let sepulcra = crate::run::root().join("games").join("sepulcra").join("assets");
    for name in COPIED {
        copy_tree(&sepulcra.join(name), &project.join("assets").join(name))?;
    }
    let port = cx.port;
    cx.start(&project, port, &["--lead", "lead"])?;

    // One bake: two files in one journal entry, and the frame data the 2D path plays.
    let walk = r#"{"recipe": "recipes/shambler.json", "clips": ["walk"], "directions": 4, "frames": 4, "cell": 48}"#;
    let baked = cx.call(port, ann, "sprite_bake", walk)?;
    let files = at(&baked, "files").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
    let paths: Vec<&str> = files.iter().filter_map(|f| text(f, "path")).collect();
    expect(paths == ["assets/sprites/shambler.png", "assets/sprites/shambler.json"], "the bake did not write its atlas and its frame data", &baked)?;
    expect(files.iter().all(|f| text(f, "disk") == Some("written")), "a file of the bake is not on disk", &baked)?;
    let journal = at(&baked, "journal").and_then(Json::as_u64).ok_or_else(|| format!("the bake names no journal entry: {}", super::short(&baked)))?;
    expect(at(&baked, "cells").and_then(Json::as_u64) == Some(16), "the bake is not 16 cells", &baked)?;
    let read = cx.call(port, ann, "project_read", r#"{"path": "assets/sprites/shambler.json"}"#)?;
    let frames = text(&read, "text").and_then(|t| Json::parse(t).ok()).ok_or_else(|| format!("the frame data did not read as JSON: {}", super::short(&read)))?;
    let sprite = at(&frames, "strips.0.sprite").ok_or_else(|| format!("the frame data has no sprite object: {}", super::short(&frames)))?.to_string();
    expect(text(&frames, "texture") == Some("sprites/shambler.png") && sprite.contains("\"anim\""), "the frame data does not play the atlas", &frames)?;
    // Played in the world as written.
    let spawned = cx.call(port, ann, "world_spawn", &format!(r#"{{"shape": "sprite", "name": "walker", "sprite": {sprite}}}"#))?;
    cx.say(format!("spawned from the frame data: {}", super::short(&spawned)));
    cx.call(port, ann, "world_delete", r#"{"path": "/walker"}"#)?;
    // world_undo of the bake's entry takes both files back.
    let undone = cx.call(port, ann, "world_undo", &format!(r#"{{"ids": [{journal}]}}"#))?;
    let left = sprites(cx, port, ann)?;
    expect(left.is_empty(), &format!("world_undo of entry {journal} left {left:?}"), &undone)?;

    // The project's light.
    let lit = cx.call(port, ann, "sprite_bake", r#"{"recipe": "recipes/statue_angel.json", "directions": 1, "cell": 32, "light": "project", "path": "assets/sprites/lit.png"}"#)?;
    expect(text(&lit, "light").is_some_and(|l| l.starts_with("project")), "the bake was not lit by the project's style.toml", &lit)?;

    // A long bake of ann's, from a process of its own; bob's cancel is refused naming ann, and
    // ann's cancel from another connection stops it.
    let (exe, p) = (cx.exe.to_string_lossy().into_owned(), port.to_string());
    // (The heaviest bake a call takes: 512 cells of 112 at supersample 4. A cell of 128 would be
    // refused at once, its atlas past 7 Mi pixels: the first run of this check met that.)
    let long = r#"{"recipe": "recipes/shambler.json", "clips": ["walk"], "directions": 16, "frames": 32, "cell": 112, "supersample": 4, "path": "assets/sprites/long.png"}"#;
    let running = Command::new(&exe)
        .args(["call", "sprite_bake", long, "--port", &p, "--settings", &settings, "--as", "ann", "--role", "builder"])
        .current_dir(&cx.out)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("the long bake's call could not be started: {e}"))?;
    // (engine_info lists the step jobs only in its detailed answer.)
    let began = std::time::Instant::now();
    let listed = loop {
        let info = cx.call(port, Some(("lead", "lead")), "engine_info", r#"{"response_format": "detailed"}"#)?;
        if at(&info, "workers.step_jobs").and_then(Json::as_array).is_some_and(|j| j.iter().any(|s| text(s, "kind") == Some("sprite_bake"))) {
            break info;
        }
        if began.elapsed() > Duration::from_secs(60) {
            return Err(format!("the long bake's step job was not queued within 60 s; the last answer was: {}", super::short(&info)));
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    cx.say(format!("queued: {}", super::short(at(&listed, "workers.step_jobs").unwrap_or(&Json::Null))));
    let refused = cx.refused(port, bob, "sprite_bake", r#"{"cancel": true}"#, "not_job_owner")?;
    expect(at(&refused, "owners").and_then(Json::as_array).is_some_and(|o| o.iter().any(|n| n.as_str() == Some("ann"))), "bob's refused cancel does not name ann", &refused)?;
    let cancelled = cx.call(port, ann, "sprite_bake", r#"{"cancel": true}"#)?;
    expect(at(&cancelled, "cancelled").and_then(Json::as_u64) == Some(1), "ann's cancel did not stop her bake", &cancelled)?;
    let out = running.wait_with_output().map_err(|e| format!("the long bake's call: {e}"))?;
    let answer = Json::parse(String::from_utf8_lossy(&out.stdout).trim()).map_err(|e| format!("the long bake's call did not answer JSON: {e}"))?;
    expect(text(&answer, "error") == Some("cancelled"), "the long bake did not answer cancelled", &answer)?;
    let left = sprites(cx, port, ann)?;
    expect(!left.iter().any(|p| p.contains("long")), &format!("a cancelled bake wrote {left:?}"), &answer)?;

    // The most cells a call takes.
    let most = r#"{"recipe": "recipes/shambler.json", "clips": ["walk"], "directions": 16, "frames": 32, "cell": 64, "path": "assets/sprites/most.png"}"#;
    // Either its sheet, or (when this machine is too busy for the call's time) the bake's own
    // `out_of_time` with how far it got, and nothing written: never a bare timeout.
    let cost = match cx.call_as(port, ann, "sprite_bake", most)? {
        Ok(full) => {
            expect(at(&full, "cells").and_then(Json::as_u64) == Some(512), "the bake of 512 cells did not answer with its sheet", &full)?;
            format!("its sheet, {}", super::short(at(&full, "cost").unwrap_or(&Json::Null)))
        }
        Err(refused) => {
            expect(text(&refused, "error") == Some("out_of_time") && at(&refused, "cells_that_fit").is_some(), "the bake of 512 cells was refused, not for the call's time", &refused)?;
            let left = sprites(cx, port, ann)?;
            expect(!left.iter().any(|p| p.contains("most")), &format!("a bake out of time wrote {left:?}"), &refused)?;
            format!("out of time, {}", super::short(&refused))
        }
    };
    cx.say(format!("512 cells, headless: {cost}"));
    cx.stop(port)?;
    Ok(format!("two files in one entry (journal {journal}) and one undo; the project's light; a bake cancelled by its owner only; 512 cells answered ({cost})"))
}

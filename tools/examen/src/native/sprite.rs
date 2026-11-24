//! `sprite_check`: `sprite_bake` end to end through a running engine, as an agent with a shell
//! uses it (docs/design/SPRITE_BAKE.md). A project made by `specula new` is given a copy of
//! Sepulcra's recipes, animations and style files, and then:
//!
//! - a shambler's walk is baked: the atlas and its frame data are two project files written in
//!   **one** journal entry, and the frame data reads as the 2D path's own sprite objects;
//! - `world_undo` takes both back at once;
//! - a bake lit by the project's light says so (`style.toml`'s `[stage]` game rig);
//! - a bake held at the engine's test gate (`SPECULA_TEST_BAKE_HOLD`, while the clock is
//!   paused: it cannot have finished, on any GPU) is a job of its owner's: another agent's
//!   cancel and question are refused naming the owner, the owner's cancel from another
//!   connection stops it, and nothing of it is written; a lead's forced cancel stops another's
//!   (another agent's force is refused `force_needs_lead`);
//! - a caller with no name (no `--as`) is not kept a bake past its call (`needs_a_name`);
//! - a cancel while the atlas is encoded and stored is honoured (nothing written), and one
//!   once its writing has begun is refused `too_late` and the files are written (the gate holds
//!   `enc.png@encoded` and `wr.png@writing`);
//! - a claim on a bake's frame data taken while it is held refuses its commit when it ends
//!   (nothing written);
//! - a bake of the most cells (512) is answered as a job and its sheet is there when it ends;
//!   the log says what it cost (the engine's own `cost` record);
//! - in an engine with a window, a bake started from a shell (whose session closes at once)
//!   uploads its model once at most (its own `cost.uploads`): review 1's finding 1.

use std::path::Path;
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
    // The gate the held bakes below wait at (a test gate of the engine's: SPRITE_BAKE.md).
    cx.engine_env("SPECULA_TEST_BAKE_HOLD", "held.png,enc.png@encoded,wr.png@writing");
    cx.start(&project, port, &["--lead", "lead"])?;

    // One bake: two files in one journal entry, and the frame data the 2D path plays.
    let walk = r#"{"recipe": "recipes/shambler.json", "clips": ["walk"], "directions": 4, "frames": 4, "cell": 48}"#;
    let baked = cx.call(port, ann, "sprite_bake", walk)?;
    let files = at(&baked, "files").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
    let paths: Vec<&str> = files.iter().filter_map(|f| text(f, "path")).collect();
    expect(paths == ["assets/sprites/shambler.png", "assets/sprites/shambler.sheet.json"], "the bake did not write its atlas and its frame data", &baked)?;
    expect(files.iter().all(|f| text(f, "disk") == Some("written")), "a file of the bake is not on disk", &baked)?;
    let journal = at(&baked, "journal").and_then(Json::as_u64).ok_or_else(|| format!("the bake names no journal entry: {}", super::short(&baked)))?;
    expect(at(&baked, "cells").and_then(Json::as_u64) == Some(16), "the bake is not 16 cells", &baked)?;
    let read = cx.call(port, ann, "project_read", r#"{"path": "assets/sprites/shambler.sheet.json"}"#)?;
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

    // A bake held by the check's own gate (`SPECULA_TEST_BAKE_HOLD`: a bake of held.png holds
    // before its first render while the clock is paused), so it cannot have finished when the
    // cancel comes, on any GPU: a job of ann's, cancelled by ann alone.
    let lead = Some(("lead", "lead"));
    cx.call(port, lead, "time_control", r#"{"action": "pause"}"#)?;
    let held = r#"{"recipe": "recipes/shambler.json", "clips": ["walk"], "directions": 4, "frames": 4, "cell": 48, "path": "assets/sprites/held.png", "wait_s": 0}"#;
    let first = cx.call(port, ann, "sprite_bake", held)?;
    let job = at(&first, "job").and_then(Json::as_u64).ok_or_else(|| format!("a bake that did not end within its wait answered no job: {}", super::short(&first)))?;
    expect(text(&first, "status") == Some("running"), "the held bake is not running", &first)?;
    let asked = cx.call(port, ann, "sprite_bake", &format!(r#"{{"job": {job}}}"#))?;
    expect(text(&asked, "phase").is_some_and(|p| p.starts_with("held")), "the bake is not held by the check's gate", &asked)?;
    let refused = cx.refused(port, bob, "sprite_bake", r#"{"cancel": true}"#, "not_job_owner")?;
    expect(at(&refused, "owners").and_then(Json::as_array).is_some_and(|o| o.iter().any(|n| n.as_str() == Some("ann"))), "bob's refused cancel does not name ann", &refused)?;
    let refused = cx.refused(port, bob, "sprite_bake", &format!(r#"{{"cancel": true, "job": {job}}}"#), "not_job_owner")?;
    expect(text(&refused, "owner") == Some("ann"), "bob's refused cancel of the job does not name ann", &refused)?;
    cx.refused(port, bob, "sprite_bake", &format!(r#"{{"job": {job}}}"#), "not_job_owner")?;
    let cancelled = cx.call(port, ann, "sprite_bake", &format!(r#"{{"cancel": true, "job": {job}}}"#))?;
    expect(at(&cancelled, "cancelled").and_then(Json::as_u64) == Some(1), "ann's cancel did not stop her bake", &cancelled)?;
    let answer = cx.refused(port, ann, "sprite_bake", &format!(r#"{{"job": {job}, "wait_s": 20}}"#), "cancelled")?;
    cx.say(format!("cancelled by its owner: {}", super::short(&answer)));
    let left = sprites(cx, port, ann)?;
    expect(!left.iter().any(|p| p.contains("held")), &format!("a cancelled bake wrote {left:?}"), &answer)?;

    // A lead's forced cancel (AUTHORITY.md, "Jobs"): bob's force is refused for want of lead
    // authority; the lead's stops ann's bake, and ann's answer says who stopped it.
    let first = cx.call(port, ann, "sprite_bake", held)?;
    let job = at(&first, "job").and_then(Json::as_u64).ok_or_else(|| format!("the held bake answered no job: {}", super::short(&first)))?;
    cx.refused(port, bob, "sprite_bake", &format!(r#"{{"cancel": true, "job": {job}, "force": true}}"#), "force_needs_lead")?;
    let listed = cx.call(port, lead, "sprite_bake", r#"{"list": true}"#)?;
    expect(at(&listed, "jobs").and_then(Json::as_array).is_some_and(|j| j.iter().any(|j| at(j, "job").and_then(Json::as_u64) == Some(job) && text(j, "owner") == Some("ann"))), "the lead's list does not name ann's bake", &listed)?;
    let forced = cx.call(port, lead, "sprite_bake", &format!(r#"{{"cancel": true, "job": {job}, "force": true}}"#))?;
    expect(at(&forced, "cancelled").and_then(Json::as_u64) == Some(1) && text(&forced, "forced") == Some("ann"), "the lead's forced cancel did not stop ann's bake", &forced)?;
    let answer = cx.refused(port, ann, "sprite_bake", &format!(r#"{{"job": {job}, "wait_s": 20}}"#), "cancelled")?;
    expect(text(&answer, "message").is_some_and(|m| m.contains("by lead")), "ann's answer does not say the lead stopped it", &answer)?;
    cx.say(format!("forced by the lead: {}", super::short(&answer)));

    // A caller with no name (no --as) is never kept a bake past its call.
    let nameless = cx.refused(port, None, "sprite_bake", r#"{"recipe": "recipes/statue_angel.json", "directions": 1, "cell": 32, "path": "assets/sprites/anon.png", "wait_s": 0}"#, "needs_a_name")?;
    cx.say(format!("no name: {}", super::short(&nameless)));

    // A cancel while the atlas is encoded and stored (held there by the gate: enc.png@encoded)
    // is honoured: cancelled, nothing written.
    let wait_phase = |cx: &mut Cx, job: u64, phase: &str| -> Result<Json, String> {
        let began = std::time::Instant::now();
        loop {
            let a = cx.call(port, ann, "sprite_bake", &format!(r#"{{"job": {job}}}"#))?;
            if text(&a, "phase").is_some_and(|p| p.starts_with(phase)) {
                return Ok(a);
            }
            if text(&a, "status") != Some("running") || began.elapsed() > Duration::from_secs(120) {
                return Err(format!("bake {job} never reached '{phase}': {}", super::short(&a)));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    let enc = cx.call(port, ann, "sprite_bake", r#"{"recipe": "recipes/statue_angel.json", "directions": 4, "cell": 32, "path": "assets/sprites/enc.png", "wait_s": 0}"#)?;
    let job = at(&enc, "job").and_then(Json::as_u64).ok_or_else(|| format!("the encoded-held bake answered no job: {}", super::short(&enc)))?;
    wait_phase(cx, job, "held")?;
    let cancelled = cx.call(port, ann, "sprite_bake", &format!(r#"{{"cancel": true, "job": {job}}}"#))?;
    expect(at(&cancelled, "cancelled").and_then(Json::as_u64) == Some(1), "a cancel while the atlas was encoded was not honoured", &cancelled)?;
    let answer = cx.refused(port, ann, "sprite_bake", &format!(r#"{{"job": {job}, "wait_s": 20}}"#), "cancelled")?;
    let left = sprites(cx, port, ann)?;
    expect(!left.iter().any(|p| p.contains("enc")), &format!("a bake cancelled once encoded wrote {left:?}"), &answer)?;

    // A cancel once its writing has begun (held there: wr.png@writing) is refused too_late,
    // and the files are written when the clock plays (below).
    let wr = cx.call(port, ann, "sprite_bake", r#"{"recipe": "recipes/statue_angel.json", "directions": 4, "cell": 32, "path": "assets/sprites/wr.png", "wait_s": 0}"#)?;
    let wr_job = at(&wr, "job").and_then(Json::as_u64).ok_or_else(|| format!("the writing-held bake answered no job: {}", super::short(&wr)))?;
    wait_phase(cx, wr_job, "writing")?;
    let late = cx.refused(port, ann, "sprite_bake", &format!(r#"{{"cancel": true, "job": {wr_job}}}"#), "too_late")?;
    cx.say(format!("too late: {}", super::short(&late)));
    let all = cx.call(port, ann, "sprite_bake", r#"{"cancel": true}"#)?;
    expect(at(&all, "cancelled").and_then(Json::as_u64) == Some(0) && at(&all, "too_late").and_then(Json::as_array).is_some_and(|t| t.len() == 1), "a cancel of all said it stopped a bake being written", &all)?;

    // A claim on its frame data that appears while the bake runs: the bake's commit is refused
    // when it ends, and nothing is written (held again, so the claim comes first by construction).
    let second = cx.call(port, ann, "sprite_bake", held)?;
    let job = at(&second, "job").and_then(Json::as_u64).ok_or_else(|| format!("the second held bake answered no job: {}", super::short(&second)))?;
    cx.call(port, bob, "claim_take", r#"{"path": "/project/assets/sprites/held.sheet.json"}"#)?;
    cx.call(port, lead, "time_control", r#"{"action": "resume"}"#)?;
    let answer = cx.refused(port, ann, "sprite_bake", &format!(r#"{{"job": {job}, "wait_s": 20}}"#), "claimed")?;
    cx.say(format!("claimed while it ran: {}", super::short(&answer)));
    let left = sprites(cx, port, ann)?;
    expect(!left.iter().any(|p| p.contains("held")), &format!("a bake refused at its commit wrote {left:?}"), &answer)?;
    cx.call(port, bob, "claim_release", r#"{"all": true}"#)?;
    // The bake whose cancel came too late wrote its two files once the clock played.
    let written = cx.call(port, ann, "sprite_bake", &format!(r#"{{"job": {wr_job}, "wait_s": 20}}"#))?;
    let left = sprites(cx, port, ann)?;
    expect(left.iter().any(|p| p == "assets/sprites/wr.png") && left.iter().any(|p| p == "assets/sprites/wr.sheet.json"), &format!("the bake refused a late cancel did not write its files: {left:?}"), &written)?;

    // The most cells a bake takes (512), as a job: its sheet when it ends, whatever this
    // machine's pace (never refused for being long).
    let most = r#"{"recipe": "recipes/shambler.json", "clips": ["walk"], "directions": 16, "frames": 32, "cell": 64, "path": "assets/sprites/most.png", "wait_s": 0}"#;
    let started = cx.call(port, ann, "sprite_bake", most)?;
    let full = match at(&started, "job").and_then(Json::as_u64) {
        None => started,
        Some(job) => {
            let began = std::time::Instant::now();
            loop {
                let a = cx.call(port, ann, "sprite_bake", &format!(r#"{{"job": {job}, "wait_s": 20}}"#))?;
                if text(&a, "status") != Some("running") {
                    break a;
                }
                if began.elapsed() > Duration::from_secs(600) {
                    return Err(format!("the bake of 512 cells had not ended after 600 s: {}", super::short(&a)));
                }
            }
        }
    };
    expect(at(&full, "cells").and_then(Json::as_u64) == Some(512), "the bake of 512 cells did not answer with its sheet", &full)?;
    let left = sprites(cx, port, ann)?;
    expect(left.iter().any(|p| p == "assets/sprites/most.png") && left.iter().any(|p| p == "assets/sprites/most.sheet.json"), &format!("the bake of 512 cells wrote {left:?}"), &full)?;
    let cost = super::short(at(&full, "cost").unwrap_or(&Json::Null));
    cx.say(format!("512 cells, headless: {cost}"));
    cx.stop(port)?;

    // Review 1, finding 1: in an engine with a window, a bake started from a shell (its
    // session closed at once) lost its pin within a second and uploaded the model again for
    // every cell. Its own count says how many times it uploaded the build: once at most.
    let wport = port + 1;
    cx.start_window(&project, wport, &["--lead", "lead"])?;
    let shell = r#"{"recipe": "recipes/shambler.json", "clips": ["walk"], "directions": 8, "frames": 8, "cell": 64, "path": "assets/sprites/win.png", "wait_s": 0}"#;
    let started = cx.call(wport, ann, "sprite_bake", shell)?;
    let windowed = match at(&started, "job").and_then(Json::as_u64) {
        None => started,
        Some(job) => {
            let began = std::time::Instant::now();
            loop {
                let a = cx.call(wport, ann, "sprite_bake", &format!(r#"{{"job": {job}, "wait_s": 20}}"#))?;
                if text(&a, "status") != Some("running") {
                    break a;
                }
                if began.elapsed() > Duration::from_secs(600) {
                    return Err(format!("the windowed bake had not ended after 600 s: {}", super::short(&a)));
                }
            }
        }
    };
    let uploads = at(&windowed, "cost.uploads").and_then(Json::as_u64);
    expect(at(&windowed, "cells").and_then(Json::as_u64) == Some(64) && uploads.is_some_and(|u| u <= 1), "the windowed bake started from a shell uploaded its model more than once (or did not end)", &windowed)?;
    let wcost = super::short(at(&windowed, "cost").unwrap_or(&Json::Null));
    cx.say(format!("64 cells, windowed, from a shell: {wcost}"));
    // A textured model (the ruined wall's stone is a texture graph): its textures leave the
    // CPU when uploaded, so a second upload would fail (`render_unavailable` midway). From a
    // shell, windowed, it bakes whole. (Its 16 cells end within the call's wait.)
    let textured = cx.call(wport, ann, "sprite_bake", r#"{"recipe": "recipes/ruined_wall.json", "directions": 16, "cell": 48, "path": "assets/sprites/wall.png"}"#)?;
    expect(at(&textured, "cells").and_then(Json::as_u64) == Some(16) && at(&textured, "cost.uploads").and_then(Json::as_u64).is_some_and(|u| u <= 1), "the textured bake from a shell did not bake whole with one upload", &textured)?;
    cx.say(format!("textured, windowed, from a shell: {}", super::short(at(&textured, "cost").unwrap_or(&Json::Null))));
    cx.stop(wport)?;
    Ok(format!("two files in one entry (journal {journal}) and one undo; the project's light; a held bake cancelled by its owner only, and by a lead's force; no bake kept for a caller with no name; a cancel honoured once encoded and refused too_late once writing; a claim that came while a bake ran refused its commit; 512 cells answered as a job ({cost}); a windowed bake from a shell uploaded its model once ({wcost})"))
}

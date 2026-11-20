//! `light_window_check`: a `perf_light` report with a window (docs/design/LIGHT.md, the fourth
//! and fifth rounds), through a running engine as an agent with a shell uses it, on each
//! adapter. The engine's window is held to 20 frames a second (`--max-fps 20`) and opened
//! behind the others (`--no-activate`); a small lit scene is made, a report is asked for, and
//! what comes back is held to what the design says:
//!
//! - while the report's render is on the graphics card the window's serve wait looks at it
//!   (`perf_frame`'s `serve.step_pulses` moved over the report);
//! - so the report lands more than one render a frame: its renders outnumber the frames the
//!   window presented from just before the call to just after it (a count, not a time; the
//!   frames of the commands around the call are counted against it);
//! - nothing the validation layer said: the view's own capture after the report has no
//!   validation warning, and the engine says at its end that it saw no validation error.
//!
//! Before the fourth round a job was looked at once a frame, so it landed one render a frame,
//! and this check failed on both counts.

use std::path::Path;

use specula_core::Json;

use super::{at, expect, text, Cx};

/// The scene: three cubes on a floor, with every light feature the report weighs switched on.
const SETUP: [(&str, &str); 5] = [
    ("world_env", r#"{"shadow": {"enabled": true}, "ssao": {"enabled": true}, "bloom": {"enabled": true}, "fxaa": true, "fog": {"enabled": true}, "sky": {"enabled": true}}"#),
    ("world_spawn", r#"{"shape": "quad", "name": "floor", "scale": [8, 1, 8]}"#),
    ("world_spawn", r#"{"shape": "cube", "name": "c1", "position": [0, 0.5, 0]}"#),
    ("world_spawn", r#"{"shape": "cube", "name": "c2", "position": [1.5, 0.5, 0]}"#),
    ("world_spawn", r#"{"shape": "cube", "name": "c3", "position": [-1.5, 0.5, 0]}"#),
];

const REPORT: &str = r#"{"width": 640, "height": 360, "frames": 9, "eye": [4, 3, 6], "target": [0, 0.5, 0]}"#;

fn number(json: &Json, path: &str) -> Option<f64> {
    at(json, path).and_then(Json::as_f64)
}

pub fn light_window(cx: &mut Cx) -> Result<String, String> {
    let lead = Some(("lead", "lead"));
    let settings = cx.settings();
    let projects = cx.out.join("p");
    let project = projects.join("lightwin");
    if project.exists() {
        return Err(format!("{} is there already: give the check an output folder of its own (it never deletes one)", project.display()));
    }
    let (code, made) = cx.cli(&["new", "lightwin", "--in", &projects.to_string_lossy(), "--settings", &settings])?;
    expect(code == 0, "`specula new` did not make the project", &made)?;
    let mut said = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (i, adapter) in ["integrated", "discrete"].into_iter().enumerate() {
        let port = cx.port + i as u64;
        let log = cx.start_window(&project, port, &["--lead", "lead", "--adapter", adapter, "--max-fps", "20", "--size", "960x540"])?;
        let info = cx.call(port, lead, "engine_info", "{}")?;
        let name = text(&info, "gpu.name").unwrap_or("?").to_owned();
        expect(at(&info, "validation").and_then(Json::as_bool) == Some(true), "the validation layer is not on (install the Vulkan SDK for a meaningful check)", &info)?;
        for (tool, args) in SETUP {
            cx.call(port, lead, tool, args)?;
        }
        // The window presents a while before the report (its rate and the scene's first frames).
        std::thread::sleep(std::time::Duration::from_secs(2));
        let before = cx.call(port, lead, "perf_frame", "{}")?;
        let report = cx.call(port, lead, "perf_light", REPORT)?;
        let after = cx.call(port, lead, "perf_frame", "{}")?;
        let read = |j: &Json, key: &str| number(j, &format!("serve.{key}"));
        let (Some(p0), Some(p1), Some(w0), Some(w1)) = (read(&before, "step_pulses"), read(&after, "step_pulses"), read(&before, "windows"), read(&after, "windows")) else {
            return Err(format!("perf_frame has no serve record with step_pulses and windows: {}", super::short(&after)));
        };
        expect(read(&after, "max_fps") == Some(20.0), "the window is not held to 20 frames a second", &after)?;
        let renders = number(&report, "renders").ok_or_else(|| format!("the report has no renders: {}", super::short(&report)))?;
        expect(at(&report, "partial").is_none(), "the report was cut short (its call's time ran out)", &report)?;
        let (pulses, frames) = (p1 - p0, w1 - w0);
        if pulses < 1.0 || renders <= frames {
            return Err(format!(
                "on the {adapter} adapter ({name}) the report's {renders} renders landed over {frames} frames of the window, with {pulses} pulses (serve before: {}; after: {})",
                super::short(at(&before, "serve").unwrap_or(&Json::Null)),
                super::short(at(&after, "serve").unwrap_or(&Json::Null))
            ));
        }
        // The validation layer: nothing in the view's own capture, and no error at the end.
        let frame = cx.call(port, lead, "sight_frame", r#"{"width": 320, "height": 180, "response_format": "detailed"}"#)?;
        let warned: Vec<String> = at(&frame, "warnings").and_then(Json::as_array).map(|w| w.iter().map(super::short).filter(|s| s.to_lowercase().contains("validation")).collect()).unwrap_or_default();
        expect(warned.is_empty(), "the validation layer warned", &frame)?;
        cx.stop(port)?;
        let ended = std::fs::read_to_string(&log).map_err(|e| format!("{}: {e}", log.display()))?;
        let last = ended.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").to_owned();
        if !last.contains("shut down (0 validation errors)") {
            return Err(format!("the engine on the {adapter} adapter did not end saying it saw no validation error; its last line: {last} ({})", Path::new(&log).display()));
        }
        let landing = at(&after, "serve.step_landing_ms").map_or_else(|| "?".to_owned(), super::short);
        cx.say(format!("{adapter} ({name}): {renders} renders over {frames} frames, {pulses} pulses, a landing charged {landing} ms; report took {} ms", number(&report, "took_ms").unwrap_or(-1.0)));
        said.push(format!("{adapter} ({name}): {renders} renders over {frames} frames, {pulses} pulses"));
        seen.push(name);
    }
    let one = if seen.len() == 2 && seen[0] == seen[1] { " (one adapter on this machine: both runs used it)" } else { "" };
    Ok(format!("a report with a window at 20 fps lands more than a render a frame, the serve wait looking at it; no validation message: {}{one}", said.join("; ")))
}

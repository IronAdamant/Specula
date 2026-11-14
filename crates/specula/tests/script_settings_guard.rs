//! No script runs a hosted game on the real per-user settings of whoever runs the script.
//!
//! A windowed game without `--settings` reads the player's own settings file and writes what the
//! window's input changes back into it (docs/design/ACCESSIBILITY.md). A script's engine is not
//! the player: a saved "reduced motion" would skip an intro a check waits for, a saved interface
//! size would change a budget a check measures, and the keys and clicks a script posts to the
//! window would be saved as the player's. (Review R1 of m5-a11y, finding 5: only one script
//! passed the flag.)
//!
//! So `scripts/specula_mcp.py`'s `Engine` adds `--settings off` unless the caller names a folder,
//! and every script that starts an engine process itself passes the flag. This test holds both.

use std::path::Path;

/// Scripts that start a process which isn't the engine, and what it is.
const NOT_AN_ENGINE: &[(&str, &str)] = &[("m3_exit.py", "the PowerShell GPU-load sampler"), ("gate.py", "cargo (tests, clippy and builds): it starts no game"),
    ("leverage.py", "an agent's own command line (settings.json's runner: a builder or a judge of the measured comparison), never a game; its engines are started by `specula open` with the run's --settings folder and --headless"),
    ("launch_check.py", "the executable's own commands (specula projects, open, call...), each given --settings; the engines `specula open` starts are headless or open with --no-activate"),
];

/// How many lines after `subprocess.Popen(` the flag may stand on (the argument list).
const WINDOW: usize = 8;

#[test]
fn scripts_never_run_a_game_on_the_real_per_user_settings() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
    let helper = std::fs::read_to_string(dir.join("specula_mcp.py")).expect("scripts/specula_mcp.py");
    assert!(
        helper.contains(r#"self.args[0:0] = ["--settings", "off"]"#) && helper.contains(r#"if "--settings" not in engine_args"#),
        "scripts/specula_mcp.py: Engine must add --settings off unless the caller passes --settings"
    );
    let mut bad = Vec::new();
    let mut seen = 0;
    for e in std::fs::read_dir(&dir).expect("scripts/") {
        let path = e.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.extension().is_none_or(|x| x != "py") || name == "specula_mcp.py" {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("subprocess.Popen(") {
                continue;
            }
            seen += 1;
            if NOT_AN_ENGINE.iter().any(|(n, _)| *n == name) {
                continue;
            }
            let args = lines[i..(i + WINDOW).min(lines.len())].join("\n");
            if !args.contains("\"--settings\"") {
                bad.push(format!("scripts/{name}:{}", i + 1));
            }
            // And an engine with a window takes no keyboard from whoever is using the machine
            // (review R3 of m5-eddraw, F4: window_check.py started one that did).
            if !args.contains("\"--headless\"") && !args.contains("\"--no-activate\"") {
                bad.push(format!("scripts/{name}:{}: a windowed engine without \"--no-activate\"", i + 1));
            }
        }
    }
    assert!(seen >= 2, "the guard found {seen} process launches: has the way scripts start engines changed?");
    assert!(
        bad.is_empty(),
        "these start a process without --settings (pass \"--settings\", \"off\", or use specula_mcp.Engine; if it isn't an engine, name it in NOT_AN_ENGINE):\n{}",
        bad.join("\n")
    );
    for (name, what) in NOT_AN_ENGINE {
        assert!(dir.join(name).is_file() && !what.is_empty(), "NOT_AN_ENGINE names scripts/{name}, which is gone");
    }
}

/// What a script may never do to the machine of whoever runs it: move their mouse pointer, press
/// their mouse or keyboard for them, or take the keyboard from the window they are in. (Owner,
/// 2026-11-13: "I can see the agent moving my cursor around while I'm reading". The live checks
/// parked the real pointer beside the engine's window before every injected input.)
///
/// Scripts speak to the engine's own window only: with messages posted to it, and through the
/// protocol. A pointer over the engine's window is handled by moving the window
/// (`specula_mcp.clear_of_pointer`), never the pointer.
const HANDS_OFF: &[(&str, &str)] = &[
    ("SetCursorPos", "moves the human's mouse pointer"),
    ("mouse_event", "presses the human's mouse"),
    ("SendInput", "types and clicks as the human, into whatever window they are in"),
    ("keybd_event", "types on the human's keyboard"),
    ("SetForegroundWindow", "takes the keyboard from the window the human is in"),
    ("BringWindowToTop", "raises a window over what the human is looking at"),
    ("SetActiveWindow", "takes the keyboard from the window the human is in"),
    ("SwitchToThisWindow", "takes the keyboard from the window the human is in"),
    ("BlockInput", "locks the human's mouse and keyboard"),
    ("ClipCursor", "traps the human's mouse pointer"),
];

/// Modules that drive the real mouse and keyboard.
const MODULES: &[(&str, &str)] = &[
    ("pyautogui", "drives the human's mouse and keyboard"),
    ("pynput", "drives the human's mouse and keyboard"),
    ("pydirectinput", "drives the human's mouse and keyboard"),
    ("import keyboard", "drives the human's keyboard"),
    ("import mouse", "drives the human's mouse"),
];

/// Every `.py` file under `dir`, to `depth` folders down.
fn python_files(dir: &Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = e.path();
        if path.is_dir() && depth > 0 {
            python_files(&path, depth - 1, out);
        } else if path.extension().is_some_and(|x| x == "py") {
            out.push(path);
        }
    }
}

#[test]
fn scripts_keep_their_hands_off_the_humans_pointer_keyboard_and_focus() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
    let mut files = Vec::new();
    python_files(&dir, 3, &mut files);
    let mut bad = Vec::new();
    for path in &files {
        let name = path.strip_prefix(&dir).unwrap_or(path).to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(path).unwrap();
        for (i, line) in text.lines().enumerate() {
            // The name anywhere on the line: a call, an alias (`f = user32.SetCursorPos`), a
            // `getattr`, behind a `#` that sits inside a string. A line that only talks about
            // one says so with the marker. (Review R3 of m5-eddraw, F7: the first form of this
            // guard read `name(` after cutting the line at its first `#`.)
            if line.contains("hands-off: prose") {
                continue;
            }
            for (word, what) in HANDS_OFF.iter().chain(MODULES) {
                if line.contains(word) {
                    bad.push(format!("scripts/{name}:{}: {word} {what}", i + 1));
                }
            }
        }
    }
    assert!(files.len() >= 10, "the guard read {} scripts: has scripts/ moved?", files.len());
    assert!(
        bad.is_empty(),
        "a script never touches the human's pointer, keyboard or focus (post messages to the engine's own window; move the engine's window, specula_mcp.clear_of_pointer; a line that only mentions one carries `hands-off: prose`):\n{}",
        bad.join("\n")
    );
    let helper = std::fs::read_to_string(dir.join("specula_mcp.py")).expect("scripts/specula_mcp.py");
    assert!(helper.contains("def clear_of_pointer(") && helper.contains("0x0001 | 0x0004 | 0x0010"), "clear_of_pointer moves the engine's window without sizing, raising or activating it");
    // A script's engine opens its window behind the others, taking no keyboard.
    assert!(
        helper.contains(r#"self.args[0:0] = ["--no-activate"]"#) && helper.contains(r#"if "--no-activate" not in engine_args"#),
        "scripts/specula_mcp.py: Engine must add --no-activate to a windowed engine unless the caller passes it"
    );
}

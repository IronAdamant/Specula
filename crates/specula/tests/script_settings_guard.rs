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
const NOT_AN_ENGINE: &[(&str, &str)] = &[("m3_exit.py", "the PowerShell GPU-load sampler"),
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

/// Every text file a script reads or writes names its encoding. The engine writes UTF-8 and the
/// assets are UTF-8; Python's default on Windows is the system's code page, so a script that
/// said nothing read a capture's packet as cp1252 and stopped on one character outside it (M5's
/// final audit: `map1_check`), and would have written a translated string as other bytes than
/// the engine reads. `read_text`, `write_text` and the built-in `open` of a text file carry
/// `encoding=` within the call (here: on its line or the next three), and so does a process
/// whose output is read as text (`text=True`: here, within three lines either side).
#[test]
fn scripts_name_the_encoding_of_the_text_they_read_and_write() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
    let mut bad = Vec::new();
    let mut seen = 0;
    let mut seen_processes = 0;
    for entry in std::fs::read_dir(&dir).expect("scripts/") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "py") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let code = line.split('#').next().unwrap_or("");
            let plain_open = code.match_indices("open(").any(|(at, _)| {
                // The built-in: not a method (`.open(`), not the end of another name (`urlopen(`, `Popen(`).
                code[..at].chars().next_back().is_none_or(|c| !(c == '.' || c == '_' || c.is_ascii_alphanumeric()))
            });
            if code.contains("text=True") || code.contains("universal_newlines=True") {
                seen_processes += 1;
                let call = lines[i.saturating_sub(3)..(i + 4).min(lines.len())].join("\n");
                if !call.contains("encoding=") {
                    bad.push(format!("scripts/{name}:{}: {}", i + 1, line.trim()));
                }
            }
            if !(code.contains(".read_text(") || code.contains(".write_text(") || plain_open) {
                continue;
            }
            seen += 1;
            let call = lines[i..(i + 4).min(lines.len())].join("\n");
            let binary = ["\"rb\"", "\"wb\"", "\"ab\"", "'rb'", "'wb'", "'ab'"].iter().any(|m| call.contains(m));
            if !call.contains("encoding=") && !binary {
                bad.push(format!("scripts/{name}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(seen >= 200, "the guard found {seen} text reads and writes: has the way scripts read files changed?");
    assert!(seen_processes >= 20, "the guard found {seen_processes} processes read as text: has the way scripts run them changed?");
    assert!(bad.is_empty(), "these read or write text in the system's default encoding (add encoding=\"utf-8\"):\n{}", bad.join("\n"));
}

/// Scripts reach an engine through `specula_mcp`'s `urlopen`, `connect` and `Connection`, which
/// connect first and set the timeout after. Python's connect with a timeout, on Windows, often
/// takes 10 to 26 ms where a connect takes 0.1 ms (about one call in seven; the measurements are
/// in `specula_mcp.Connection`), and a check that times a call reads that wait as the engine's:
/// M5's final audit failed on it (`entity_refs_check`: a `note_list` at 21.9 ms that the engine's
/// record put at 0.1 ms). The one connect that keeps its timeout is `specula_mcp.poll`, for
/// loops that wait for a port.
#[test]
fn scripts_connect_to_an_engine_before_they_set_a_timeout() {
    // (Every spelling that reaches the same calls: an import of the name, an alias of the module.)
    const OWN: [&str; 9] = [
        "urllib.request.urlopen(",
        "create_connection(",
        "HTTPConnection(",
        "HTTPSConnection(",
        "from urllib.request import",
        "from urllib import request",
        "import urllib.request as",
        "from socket import",
        "from http.client import",
    ];
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
    let mut bad = Vec::new();
    let mut scripts = 0;
    for entry in std::fs::read_dir(&dir).expect("scripts/") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "py") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        scripts += 1;
        if name == "specula_mcp.py" {
            // The helpers themselves: the connect takes no timeout, and one poll does.
            let helper = text.split("class Connection(http.client.HTTPConnection):").nth(1).unwrap_or("");
            let connect = helper.split("class _Handler").next().unwrap_or("");
            // (Read by lines: the file ends its lines as its checkout does. The first form of
            // this check asked for the call followed by "\n", passed in the tree it was written
            // in and failed in every fresh checkout on Windows.)
            let lines: Vec<&str> = connect.lines().map(str::trim).collect();
            assert!(
                lines.contains(&"self.sock = socket.create_connection((self.host, self.port))") && lines.contains(&"self.sock.settimeout(self.timeout)"),
                "specula_mcp.Connection.connect connects with no timeout and sets it after"
            );
            assert_eq!(text.matches("urllib.request.urlopen(").count(), 1, "specula_mcp.py: only `poll` opens a URL with a timeout on its connect");
            assert!(text.contains("def poll(req):") && text.contains("return urllib.request.urlopen(req, timeout=1)"), "specula_mcp.poll is the one");
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            let code = line.split('#').next().unwrap_or("");
            if OWN.iter().any(|own| code.contains(own)) {
                bad.push(format!("scripts/{name}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    // (A floor that says the folder was found, not a count: scripts leave as each is ported to
    // Rust, tools/examen; this guard goes with the last of them.)
    assert!(scripts >= 40, "the guard read {scripts} scripts: has scripts/ moved?");
    assert!(
        bad.is_empty(),
        "these open their own connection to an engine (use specula_mcp's urlopen, connect, Connection or poll):\n{}",
        bad.join("\n")
    );
}

// (The gate and the audit's runner are Rust since 2026-11-16, `tools/examen`: their own tests
// are that crate's unit tests and run with the workspace's. The test that ran the two Python
// scripts' `--self-test` here went with them.)

/// A script that edits one of the repository's own sources and puts it back handles it as
/// bytes. Python on Windows writes text with CRLF line ends, so a source read and restored as
/// text comes back changed in a checkout whose files are LF, and a comparison made as text
/// says it is the same: M5's last audit passed with `games/ambulo/src/lib.rs` left that way by
/// `trace_check.py`. The audit's runner looks too, after every check, at what git says a
/// check left behind (`tools/examen/src/live.rs`, `tree_state`): this test holds the form that
/// caused it and that the runner still looks.
#[test]
fn scripts_put_a_source_of_the_repository_back_byte_for_byte() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
    let mut bad = Vec::new();
    let mut sources = 0;
    for entry in std::fs::read_dir(&dir).expect("scripts/") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "py") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        // A name given to a Rust source under the repository's root: `SRC = ROOT / ... / "lib.rs"`.
        let named: Vec<&str> = text
            .lines()
            .filter_map(|line| line.split_once(" = ROOT / "))
            .filter(|(_, rest)| rest.trim_end().ends_with(".rs\""))
            .map(|(name, _)| name.trim())
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            .collect();
        sources += named.len();
        for (i, line) in text.lines().enumerate() {
            let code = line.split('#').next().unwrap_or("");
            for n in &named {
                if code.contains(&format!("{n}.write_text(")) || code.contains(&format!("{n}.read_text(")) || code.contains(&format!("open({n}")) || code.contains(&format!("{n}.open(")) {
                    bad.push(format!("scripts/{name}:{}: {}", i + 1, line.trim()));
                }
            }
        }
    }
    assert!(sources >= 4, "the guard found {sources} scripts' names for a source of the repository: has their form changed?");
    assert!(bad.is_empty(), "these handle a source of the repository as text (read_bytes and write_bytes keep its line ends):\n{}", bad.join("\n"));
    let runner = std::fs::read_to_string(dir.join("../tools/examen/src/live.rs")).expect("tools/examen/src/live.rs");
    let lines: Vec<&str> = runner.lines().map(str::trim).collect();
    assert!(
        runner.contains(r#"&["git", "status", "--porcelain", "-uall"]"#)
            && runner.contains(r#"&["git", "diff", "HEAD"]"#)
            && lines.contains(&"let left = left_behind(tree.as_ref(), now.as_ref());")
            && lines.contains(&"let passed = ran.exit.ok() && left.is_empty();"),
        "tools/examen/src/live.rs fails a check that leaves the checkout changed"
    );
}

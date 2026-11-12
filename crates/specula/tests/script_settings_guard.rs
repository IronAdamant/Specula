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
const NOT_AN_ENGINE: &[(&str, &str)] = &[("m3_exit.py", "the PowerShell GPU-load sampler")];

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

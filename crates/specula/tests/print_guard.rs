//! Engine output goes through the trace (docs/design/TRACE.md), not ad-hoc console output: a
//! source file in an engine crate (`crates/*/src`) or a game (`games/*/src`) may not gain a
//! `println!`, `eprintln!`, `print!`, `eprint!`, `dbg!`, or a direct `io::stdout()` /
//! `io::stderr()` (how `writeln!` reaches the console). The ones there when tracing landed are
//! allowed (phase 2 migrates them to events), so the count only goes down. When you remove some,
//! lower the file's allowance here (the test says which).
//!
//! Comment lines are skipped. For a human-facing message, emit a trace event (stderr shows `warn`
//! and up). Allowed on purpose: `trace_sink.rs`'s one `io::stderr()` (it is the stderr sink).

use std::path::{Path, PathBuf};

/// `(file, outputs allowed)`: the files that had console output on 2026-10-29.
const ALLOWED: [(&str, usize); 62] = [
    ("crates/specula/src/capture.rs", 3),
    ("crates/specula/src/disk.rs", 4),
    ("crates/specula/src/evidence.rs", 5),
    ("crates/specula/src/fpguard.rs", 1),
    ("crates/specula/src/leases.rs", 1),
    ("crates/specula/src/lex_live.rs", 5),
    ("crates/specula/src/lib.rs", 28),
    ("crates/specula/src/logic/mod.rs", 1),
    ("crates/specula/src/logic/tests.rs", 1),
    ("crates/specula/src/luts.rs", 2),
    ("crates/specula/src/main.rs", 1),
    ("crates/specula/src/meshes.rs", 10),
    ("crates/specula/src/models.rs", 10),
    ("crates/specula/src/nav.rs", 1),
    ("crates/specula/src/persist.rs", 1),
    ("crates/specula/src/physics.rs", 2),
    ("crates/specula/src/playtest_engine.rs", 3),
    ("crates/specula/src/project.rs", 2),
    ("crates/specula/src/project_engine.rs", 3),
    ("crates/specula/src/project_text.rs", 1),
    ("crates/specula/src/review_screen.rs", 1),
    ("crates/specula/src/safe_path.rs", 2),
    ("crates/specula/src/schedule.rs", 7),
    ("crates/specula/src/sight.rs", 1),
    ("crates/specula/src/swarm_engine.rs", 1),
    ("crates/specula/src/tools.rs", 1),
    ("crates/specula/src/toolscope_tools.rs", 3),
    // A GPU test's reasons for skipping (rule 7: say why, then pass).
    ("crates/specula/src/trace_engine.rs", 2),
    ("crates/specula/src/trace_sink.rs", 1),
    ("crates/specula/src/two_d.rs", 3),
    ("crates/specula/src/vfx.rs", 1),
    ("crates/specula/src/write_policy.rs", 7),
    ("crates/specula-core/src/dmath/tests.rs", 8),
    ("crates/specula-core/src/jpeg/tests.rs", 1),
    ("crates/specula-core/src/png.rs", 2),
    ("crates/specula-core/src/profile.rs", 1),
    ("crates/specula-gpu/src/gpu.rs", 10),
    ("crates/specula-gpu/src/graph.rs", 2),
    ("crates/specula-gpu/src/idbuffer.rs", 4),
    ("crates/specula-gpu/src/sprite.rs", 1),
    ("crates/specula-gpu/src/vector.rs", 2),
    ("crates/specula-locus/src/geom.rs", 1),
    ("crates/specula-logic/src/build.rs", 1),
    ("crates/specula-logic/src/loader.rs", 1),
    ("crates/specula-model/src/csg/tests.rs", 1),
    ("crates/specula-model/src/sdf.rs", 1),
    ("crates/specula-platform/src/gamepad.rs", 2),
    ("crates/specula-platform/src/win32.rs", 3),
    ("crates/specula-protocol/src/mcp.rs", 1),
    ("crates/specula-protocol/src/server.rs", 2),
    ("crates/specula-scintilla/src/eval.rs", 4),
    ("crates/specula-scintilla/src/flash.rs", 2),
    ("crates/specula-sys/src/fpenv.rs", 3),
    ("crates/specula-sys/src/lib.rs", 2),
    ("crates/specula-text/src/font.rs", 1),
    ("crates/specula-text/src/fuzz_tests.rs", 1),
    ("games/sepulcra-app/src/ents.rs", 1),
    ("games/sepulcra-app/src/game.rs", 3),
    ("games/sepulcra-app/src/hud.rs", 2),
    ("games/sepulcra-app/src/lexmode.rs", 2),
    ("games/sepulcra-app/src/main.rs", 1),
    ("games/sepulcra-app/src/saves.rs", 1),
];

/// What counts as console output (longest first, so `eprintln!` isn't also `println!`).
const OUTPUTS: [&str; 7] = ["eprintln!", "println!", "eprint!", "print!", "dbg!", "io::stdout(", "io::stderr("];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn prints(text: &str) -> usize {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim_start().starts_with("//")) {
        let mut at = 0;
        while at < line.len() {
            let rest = &line[at..];
            let prev = line[..at].chars().next_back();
            match OUTPUTS.iter().find(|o| rest.starts_with(**o)) {
                Some(o) if !prev.is_some_and(ident) => {
                    n += 1;
                    at += o.len();
                }
                _ => at += rest.chars().next().map_or(1, char::len_utf8),
            }
        }
    }
    n
}

#[test]
fn engine_sources_gain_no_prints() {
    let root = repo();
    let mut files = Vec::new();
    for top in ["crates", "games"] {
        let Ok(dirs) = std::fs::read_dir(root.join(top)) else { panic!("no {top} folder") };
        for c in dirs.flatten() {
            rust_files(&c.path().join("src"), &mut files);
        }
    }
    assert!(files.len() > 100, "found the engine's sources ({})", files.len());
    let mut over = Vec::new();
    let mut lower = Vec::new();
    let mut total = 0;
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap_or(f).to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(f).unwrap_or_default();
        let n = prints(&text);
        total += n;
        let allowed = ALLOWED.iter().find(|(p, _)| *p == rel).map_or(0, |(_, a)| *a);
        if n > allowed {
            over.push(format!("{rel}: {n} console outputs, {allowed} allowed"));
        } else if n < allowed {
            lower.push(format!("{rel}: allowance {allowed} -> {n}"));
        }
    }
    assert!(
        over.is_empty(),
        "new console output in engine or game sources: emit a trace event instead (specula_core::trace_event!, docs/design/TRACE.md):\n{}",
        over.join("\n")
    );
    if !lower.is_empty() {
        println!("print_guard: {total} console outputs left; these allowances can come down:\n{}", lower.join("\n"));
    }
}

#[test]
fn the_counter_sees_every_form_and_skips_comments() {
    assert_eq!(prints("println!(\"a\"); eprintln!(\"b\");\n// eprintln!(\"c\")\nmy_println!(x)"), 2);
    assert_eq!(prints("print!(\"a\"); eprint!(\"b\"); dbg!(x); sprint!(y)"), 3);
    assert_eq!(prints("let mut e = std::io::stderr().lock(); writeln!(std::io::stdout(), \"x\");"), 2);
}

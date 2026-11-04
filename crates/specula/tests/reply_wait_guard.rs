//! A tool waiting for the engine's answer on a caller thread must wait for it (`recv_timeout`),
//! never peek (`try_recv`): the engine may queue a request (a project snapshot behind a sync) and
//! answer it a frame later, and a peek turns that into a spurious `no_answer` (the dropped gallery
//! reply of the M4 final audit: one `gallery_*` call in three failed back to back with another).
//!
//! Caller-thread tool code is any engine source file named `*tools*` (or under a `*tools*`
//! folder) or one that takes the caller's `Tools` handle. Engine-thread code (screens, workers,
//! the frame loop) polls with `try_recv` on purpose and is not scanned. An exception names its
//! file and its reason in [`ALLOWED`]. Comment lines are skipped.

use std::path::{Path, PathBuf};

/// `(file, try_recv calls allowed, why)`.
const ALLOWED: [(&str, usize, &str); 0] = [];

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn is_caller_thread(rel: &str, text: &str) -> bool {
    rel.split('/').any(|part| part.contains("tools")) || text.contains("&Tools") || text.contains("&crate::tools::Tools") || text.contains("impl Tools")
}

fn count(text: &str) -> usize {
    text.lines().filter(|l| !l.trim_start().starts_with("//")).map(|l| l.matches("try_recv(").count()).sum()
}

#[test]
fn caller_threads_wait_for_replies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rs_files(&root.join("crates/specula/src"), &mut files);
    for g in std::fs::read_dir(root.join("games")).unwrap().flatten() {
        if g.path().join("src").is_dir() {
            rs_files(&g.path().join("src"), &mut files);
        }
    }
    let mut bad = Vec::new();
    let mut scanned = 0;
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(f).unwrap();
        if !is_caller_thread(&rel, &text) {
            continue;
        }
        scanned += 1;
        let n = count(&text);
        let allowed = ALLOWED.iter().find(|a| a.0 == rel).map_or(0, |a| a.1);
        if n > allowed {
            bad.push(format!("{rel}: {n} try_recv (allowed {allowed}); wait with recv_timeout(project_tools::DEFERRED_WAIT) instead"));
        }
    }
    assert!(scanned >= 20, "the scan found only {scanned} caller-thread tool files: did the layout change?");
    assert!(bad.is_empty(), "caller-thread tool code peeks at a reply it should wait for:\n{}", bad.join("\n"));
}

#[test]
fn the_scan_sees_a_peek() {
    assert!(is_caller_thread("crates/specula/src/taste_tools.rs", ""));
    assert!(is_caller_thread("crates/specula/src/studio.rs", "fn f(tools: &Tools) {}"));
    assert!(!is_caller_thread("crates/specula/src/gallery_screen.rs", "rx.try_recv()"));
    assert_eq!(count("let s = rx.try_recv();\n// rx.try_recv() in a comment\n"), 1);
}

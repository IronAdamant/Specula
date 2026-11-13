//! The disk footprint budget's guard (docs/design/DISK.md): every file the engine writes goes
//! through the one accounting point, `crate::disk::book()` (admit, write, account), so a writer
//! can't fill the operator's disk unseen or past the free-space floor. This test fails when
//! engine code (and the hosted game's) writes or deletes files any other way, outside the files
//! listed here with their reason; each listed file must itself go through the book.
//!
//! Test code (`#[cfg(test)]` items, files only tests compile) is skipped. Shared lexer: the same
//! as the write policy's guard (comments and literals blanked, test items skipped by depth).

use std::path::{Path, PathBuf};

/// A raw file-system write and the files that may make it, with why.
struct Rule {
    pattern: &'static str,
    allowed: &'static [&'static str],
    why: &'static str,
}

const WRITERS: &[Rule] = &[
    // Matched as bare paths, whitespace removed across lines: a call, a function value
    // (`let f = std::fs::write;`), a turbofish or a qualified form all contain these.
    Rule { pattern: "fs::write", allowed: &["engine:disk.rs", "logic:disk.rs"], why: "writes go through disk::book().write (admitted, accounted)" },
    Rule {
        pattern: "File::create",
        allowed: &["engine:disk.rs", "engine:blobs.rs", "engine:project.rs", "engine:save_tools.rs", "engine:swarm_engine.rs", "engine:review_engine.rs"],
        why: "atomic writers (temporary file, then rename) admit with disk::book().admit and account with wrote/replaced",
    },
    Rule { pattern: "File>::", allowed: &[], why: "call File's functions by their path (File::create), so this guard sees them" },
    Rule { pattern: "File::options", allowed: &[], why: "writes go through the book" },
    Rule { pattern: "OpenOptions::new", allowed: &["engine:disk.rs", "engine:blobs.rs", "engine:persist.rs", "engine:launch_engine.rs", "engine:launch_store.rs"], why: "the content store's uploads and the world lock / world files, admitted through the book; the project's lock file opened for reading only, to hold or ask for its lock (its one write goes through the book); and a hand-over file made only if it is not there yet (admitted by the book first)" },
    Rule { pattern: "fs::copy", allowed: &["engine:disk.rs"], why: "copies are writes: through the book" },
    Rule {
        pattern: "fs::rename",
        allowed: &["engine:blobs.rs", "engine:project.rs", "engine:persist.rs", "engine:save_tools.rs", "engine:swarm_engine.rs", "engine:review_engine.rs", "engine:disk.rs", "engine:user_settings.rs", "engine:launch_store.rs"],
        why: "a rename moves bytes the budget hasn't seen: only atomic writers renaming their own admitted temporary files (and moving an unreadable file aside), and disk::move_to_trash (a folder moved within its own root, still counted there until a worker empties it)",
    },
    Rule { pattern: "fs::hard_link", allowed: &[], why: "links are writes the budget can't see: never" },
    Rule { pattern: "fs::soft_link", allowed: &[], why: "links are writes the budget can't see: never" },
    Rule { pattern: "symlink_dir", allowed: &[], why: "links are writes the budget can't see: never" },
    Rule { pattern: "symlink_file", allowed: &[], why: "links are writes the budget can't see: never" },
    Rule {
        pattern: "create_dir_all",
        allowed: &["engine:disk.rs", "engine:project.rs", "engine:persist.rs", "engine:save_tools.rs", "logic:disk.rs"],
        why: "folders are made by disk::book().make_dir or right after an admit",
    },
    Rule { pattern: "fs::create_dir", allowed: &["engine:disk.rs", "engine:project.rs", "engine:persist.rs", "engine:save_tools.rs", "logic:disk.rs"], why: "folders are made by disk::book().make_dir" },
    Rule {
        pattern: "fs::remove_dir",
        allowed: &["engine:disk.rs"],
        why: "folders are removed with disk::remove_empty_dir_inside / remove_tree_inside: inside the engine's roots, never through a link",
    },
    Rule {
        pattern: "fs::remove_file",
        allowed: REMOVERS,
        why: "files are removed by disk::remove_file_inside (pruning) or by the writer that made them, which accounts for it through the book",
    },
    // Another program could write anything: only the module build service runs one (Cargo), and
    // its output is the modules category the walker measures (and admitted before each build).
    Rule {
        pattern: "Command::new",
        allowed: &["logic:build.rs", "engine:lex_live.rs", "engine:playtest_engine.rs", "engine:models.rs", "engine:launch_run.rs"],
        why: "processes the engine starts write where they like: only Cargo (admitted before the build), the engine re-running itself for a playtest, and the command line starting an engine on a project (`specula open`: that engine admits its own writes)",
    },
    // Spelled in full, so the patterns above see every call: no imports of std::fs items or groups.
    Rule { pattern: "usestd::fs::", allowed: &[], why: "call std::fs functions by their path (fs::write), so this guard sees them" },
    Rule { pattern: "usestd::fsas", allowed: &[], why: "don't rename std::fs: this guard looks for fs::" },
    Rule { pattern: "fs::DirBuilder", allowed: &[], why: "folders are made by disk::book().make_dir" },
    Rule { pattern: "fs::$", allowed: &[], why: "a macro pasting std::fs items hides them from this guard" },
    Rule { pattern: "process::Commandas", allowed: &[], why: "don't rename Command: this guard looks for Command::new" },
];

/// Files that delete files of their own (temporary files of an atomic write, superseded world
/// files, a save game asked for, a project file deleted through the store, leftover upload
/// chunks), each accounting through the book: (crate:file).
const REMOVERS: &[&str] = &[
    "engine:disk.rs",
    "engine:blobs.rs",
    "engine:project.rs",
    "engine:persist.rs",
    "engine:save_tools.rs",
    "logic:disk.rs",
];

/// `use std::{...}` groups that import `fs` items hide them from the patterns above.
fn std_group_imports_fs(squeezed: &str) -> bool {
    let mut rest = squeezed;
    while let Some(i) = rest.find("usestd::{") {
        let group = &rest[i..];
        let end = group.find(';').unwrap_or(group.len());
        if group[..end].contains("fs") {
            return true;
        }
        rest = &rest[i + 9..];
    }
    false
}

/// Files allowed a raw write must also go through the book.
const BOOK: &str = "disk::";

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("src is readable").map(|e| e.expect("entry").path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// `text` as code only: comments, string and character literals' contents blanked (newlines
/// kept, so lines still line up). Handles `//` and nested `/* */` comments, strings over several
/// lines with escapes, raw strings (`r#"..."#`), byte strings and char literals (`'{'`, `'"'`),
/// telling them from lifetimes (`'a`).
fn code_only(text: &str) -> Vec<String> {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        let next = c.get(i + 1).copied();
        if ch == '/' && next == Some('/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if ch == '/' && next == Some('*') {
            let mut depth = 0;
            while i < c.len() {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if c[i] == '\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
            }
            continue;
        }
        // Raw strings: r"..", r#".."#, br#".."#.
        let raw_at = if ch == 'r' { Some(i + 1) } else if ch == 'b' && next == Some('r') { Some(i + 2) } else { None };
        let ident_before = i > 0 && (c[i - 1].is_alphanumeric() || c[i - 1] == '_');
        if let (Some(mut j), false) = (raw_at, ident_before) {
            let mut hashes = 0;
            while c.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if c.get(j) == Some(&'"') {
                out.push('"');
                j += 1;
                loop {
                    if j >= c.len() {
                        break;
                    }
                    if c[j] == '"' && (0..hashes).all(|h| c.get(j + 1 + h) == Some(&'#')) {
                        j += 1 + hashes;
                        break;
                    }
                    if c[j] == '\n' {
                        out.push('\n');
                    }
                    j += 1;
                }
                out.push('"');
                i = j;
                continue;
            }
        }
        if ch == '"' {
            out.push('"');
            i += 1;
            while i < c.len() {
                if c[i] == '\\' {
                    i += 2;
                    continue;
                }
                if c[i] == '"' {
                    i += 1;
                    break;
                }
                if c[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            out.push('"');
            continue;
        }
        if ch == '\'' {
            // A char literal: 'x', '\n', '\u{..}', '\''. Otherwise a lifetime: keep the quote.
            let close = if next == Some('\\') {
                (i + 2..(i + 12).min(c.len())).find(|&k| c[k] == '\'')
            } else if c.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            if let Some(k) = close {
                out.push_str("' '");
                i = k + 1;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out.split('\n').map(str::to_owned).collect()
}

/// How many lines the item starting at `code[at]` spans: to its `;` or `,` when it has no
/// body (a declaration, a field, a statement), else to the `}` closing its first `{`.
fn item_len(code: &[String], at: usize) -> usize {
    let (mut depth, mut parens) = (0i64, 0i64);
    let mut opened = false;
    for (k, l) in code[at..].iter().enumerate() {
        for ch in l.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                '(' | '[' => parens += 1,
                ')' | ']' => parens -= 1,
                _ => {}
            }
        }
        let t = l.trim_end();
        if (opened && depth <= 0) || (!opened && parens <= 0 && (t.ends_with(';') || t.ends_with(','))) {
            return k + 1;
        }
    }
    code.len() - at
}

/// The non-test code of each file: `(relative path, [(line number, code)])`, each line without
/// comments or literals' contents. Test-only items (`#[cfg(test)]` on a module, a function, a
/// field, a statement: the whole item, by brace depth) and `tests.rs` files are left out.
fn engine_code(root: &Path) -> Vec<(String, Vec<(usize, String)>)> {
    let root = root.to_path_buf();
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    let rel = |f: &Path| f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
    let texts: Vec<(String, String)> = files.iter().map(|f| (rel(f), std::fs::read_to_string(f).unwrap())).collect();
    let tests = test_only_files(&texts);
    texts.iter().filter(|(r, _)| !tests.iter().any(|t| r == t || r.starts_with(&format!("{}/", t.trim_end_matches(".rs"))))).map(|(r, t)| (r.clone(), non_test_code(t))).collect()
}

/// The files only tests compile: those a `#[cfg(test)] mod name;` declares (review round 3:
/// files were skipped by name, so a `sneaky_tests.rs` compiled into the engine went unscanned,
/// and a test module in a file not named `tests.rs` was scanned).
fn test_only_files(texts: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (rel, text) in texts {
        let code = code_only(&text.replace("\r\n", "\n"));
        // The folder a file's `mod x;` declarations live in.
        let (dir, file) = rel.rsplit_once('/').map_or(("", rel.as_str()), |(d, f)| (d, f));
        let own = if matches!(file, "lib.rs" | "main.rs" | "mod.rs") { dir.to_owned() } else { format!("{}{}", if dir.is_empty() { String::new() } else { format!("{dir}/") }, file.trim_end_matches(".rs")) };
        for (i, line) in code.iter().enumerate() {
            let t = line.trim();
            let decl = match t.strip_prefix("#[cfg(test)]") {
                Some(rest) if !rest.trim().is_empty() => rest.trim().to_owned(),
                Some(_) => code.get(i + 1).map(|n| n.trim().to_owned()).unwrap_or_default(),
                None => continue,
            };
            let decl = decl.strip_prefix("pub(crate) ").or_else(|| decl.strip_prefix("pub ")).unwrap_or(&decl).to_owned();
            if let Some(name) = decl.strip_prefix("mod ").and_then(|n| n.strip_suffix(';')) {
                let base = if own.is_empty() { name.trim().to_owned() } else { format!("{own}/{}", name.trim()) };
                out.push(format!("{base}.rs"));
                out.push(format!("{base}/mod.rs"));
            }
        }
    }
    out
}

/// One file's non-test code lines, numbered from 1 (see [`engine_code`]).
fn non_test_code(text: &str) -> Vec<(usize, String)> {
    let code = code_only(&text.replace("\r\n", "\n"));
    let mut kept = Vec::new();
    let mut i = 0;
    while i < code.len() {
        let t = code[i].trim_start();
        if let Some(rest) = t.strip_prefix("#[cfg(test)]") {
            if !rest.trim().is_empty() {
                // The item on the same line: skip from here (the attribute ends where it starts).
                i += item_len(&code, i);
                continue;
            }
            // A test-only item: skip it (and any further attributes on it) whole.
            let mut at = i + 1;
            while at < code.len() && code[at].trim_start().starts_with("#[") {
                at += 1;
            }
            i = if at < code.len() { at + item_len(&code, at) } else { at };
            continue;
        }
        if !code[i].trim().is_empty() {
            kept.push((i + 1, code[i].clone()));
        }
        i += 1;
    }
    kept
}


/// One file's code with all whitespace removed, joined across lines, and where each line starts
/// in it (so a match spanning lines, `std::fs::\n write(`, is still seen and reported).
fn squeezed(lines: &[(usize, String)]) -> (String, Vec<(usize, usize)>) {
    let mut text = String::new();
    let mut starts = Vec::with_capacity(lines.len());
    for (n, l) in lines {
        starts.push((text.len(), *n));
        text.extend(l.chars().filter(|c| !c.is_whitespace()));
    }
    (text, starts)
}

fn line_at(starts: &[(usize, usize)], at: usize) -> usize {
    starts.iter().rev().find(|(s, _)| *s <= at).map_or(0, |(_, n)| *n)
}

/// What a file breaks: `(line, pattern, why)`.
fn violations(label: &str, file: &str, lines: &[(usize, String)]) -> (Vec<(usize, &'static str, &'static str)>, bool) {
    let (text, starts) = squeezed(lines);
    // `::std::` is `std::` (a leading `::` must not hide an import or a path).
    let text = text.replace("::std::", "std::").replace("use::std", "usestd");
    let mut out = Vec::new();
    let mut raw = false;
    for r in WRITERS {
        for (at, _) in text.match_indices(r.pattern) {
            if r.allowed.contains(&format!("{label}:{file}").as_str()) {
                raw = true;
            } else {
                out.push((line_at(&starts, at), r.pattern, r.why));
            }
        }
    }
    if std_group_imports_fs(&text) {
        out.push((0, "use std::{.. fs ..}", "import std::fs items by their path, so this guard sees them"));
    }
    (out, raw)
}

fn scan(root: &Path, label: &str, problems: &mut Vec<String>) {
    for (file, lines) in engine_code(root) {
        let (found, raw) = violations(label, &file, &lines);
        for (n, pattern, why) in found {
            problems.push(format!("{label} {file}:{n}: `{pattern}` ({why})"));
        }
        // (specula-logic's own disk module goes through its HOOK.)
        if raw && !lines.iter().any(|(_, l)| l.contains(BOOK) || l.contains("HOOK")) {
            problems.push(format!("{label} {file}: writes files but never goes through the disk book"));
        }
    }
}

#[test]
fn every_disk_write_goes_through_the_book() {
    let mut problems = Vec::new();
    let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    scan(&engine, "engine", &mut problems);
    // The hosted game writes through the engine's book too (play_record), and so does the module
    // host (library copies, fixes, Cargo.lock restores) through its hook, specula_logic::disk.
    let game = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra-app/src");
    scan(&game, "sepulcra-app", &mut problems);
    let logic = Path::new(env!("CARGO_MANIFEST_DIR")).join("../specula-logic/src");
    scan(&logic, "logic", &mut problems);
    // The other crates the engine calls write nothing at all.
    for c in ["specula-studio", "specula-scintilla", "specula-model", "specula-gltf", "specula-lex", "specula-balance", "specula-audio", "specula-ui", "specula-text", "specula-anim", "specula-physics", "specula-via", "specula-mens", "specula-locus", "specula-core", "specula-gpu", "specula-protocol", "specula-input", "specula-platform"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(c).join("src");
        scan(&dir, c, &mut problems);
    }
    assert!(problems.is_empty(), "files written outside the disk book:\n{}", problems.join("\n"));
}

/// The scanner sees through comments, literals and test items, and catches spacing variants.
#[test]
fn the_scanner_sees_writes_and_skips_tests() {
    let text = "fn a() {\n    // std::fs::write(p, b)\n    let s = \"std::fs::write(\";\n    std :: fs :: write (p, b);\n}\n#[cfg(test)]\nmod tests {\n    fn t() { std::fs::write(p, b).unwrap(); }\n}\n";
    let lines = non_test_code(text);
    let (found, _) = violations("engine", "x.rs", &lines);
    let hits: Vec<usize> = found.iter().map(|(n, _, _)| *n).collect();
    assert_eq!(hits, vec![4], "{lines:?}");
}

/// Review probes (round 2): nine ways round the first scanner (a renamed import, a group import,
/// a function value, a turbofish, a qualified type, a call split over lines, a rename of a big
/// file in, a constructor value, a process writing the file). Every one is caught now.
#[test]
fn the_scanner_catches_the_review_dodges() {
    let text = r#"use std::{fs::write as w1};
use std::{fs};
fn d1(p: &std::path::Path) { w1(p, b"x").unwrap(); }
fn d2(p: &std::path::Path) { let f = std::fs::write; f(p, b"x").unwrap(); }
fn d3(p: &std::path::Path) { std::fs::write::<&std::path::Path, &[u8]>(p, b"x").unwrap(); }
fn d4(p: &std::path::Path) { <std::fs::File>::create(p).unwrap(); }
fn d5(p: &std::path::Path) { std::fs::
    write(p, b"x").unwrap(); }
fn d6(p: &std::path::Path) { std::fs::rename("C:/elsewhere/big.bin", p).unwrap(); }
fn d7(p: &std::path::Path) { let o = std::fs::OpenOptions::new; o().write(true).create(true).open(p).unwrap(); }
fn d8(p: &std::path::Path) { std::fs::remove_dir_all
    (p).unwrap(); }
fn d9(p: &std::path::Path) { std::process::Command::new("cmd").args(["/c", "fsutil", "file", "createnew"]).arg(p).arg("1000000000").status().unwrap(); }
fn control(p: &std::path::Path) { std::fs::write(p, b"x").unwrap(); }
"#;
    let lines = non_test_code(text);
    let (found, _) = violations("engine", "zz_dodge.rs", &lines);
    let lines_hit: std::collections::BTreeSet<usize> = found.iter().map(|(n, _, _)| *n).collect();
    // d1 at its renamed import (line 1), d2..d9 where each starts, the control; the group imports too.
    for n in [1, 4, 5, 6, 7, 9, 10, 11, 13, 14] {
        assert!(lines_hit.contains(&n), "line {n} not caught: {found:?}");
    }
    assert!(found.iter().any(|(_, p, _)| p.starts_with("use std::{")), "group imports of fs: {found:?}");
}

/// Review probes (round 3): four more ways round the scanner. Every one is caught now.
#[test]
fn the_scanner_catches_the_round_3_dodges() {
    let text = r#"use ::std::fs as f;
fn e1(p: &std::path::Path) { f::write(p, b"x").unwrap(); }
use std::process::Command as Run;
fn e2() { Run::new("cmd").status().unwrap(); }
fn e3(p: &std::path::Path) { std::fs::DirBuilder::new().recursive(true).create(p).unwrap(); }
macro_rules! fsx { ($f:ident) => { std::fs::$f } }
fn e4(p: &std::path::Path) { fsx!(write)(p, b"x").unwrap(); }
"#;
    let lines = non_test_code(text);
    let (found, _) = violations("engine", "zz_dodge3.rs", &lines);
    let hit: std::collections::BTreeSet<usize> = found.iter().map(|(n, _, _)| *n).collect();
    // The renamed imports (lines 1 and 3: e1 and e2 can only call through them), the builder and the macro.
    for n in [1, 3, 5, 6] {
        assert!(hit.contains(&n), "line {n} not caught: {found:?}");
    }
}

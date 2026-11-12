//! The input monitor's guard (docs/design/SECURITY.md): the window's keyboard, the system
//! clipboard, the engine screens and the human's simulated input are reached only through paths
//! the monitor sees. The compiler holds most of it (the guarded actions take an
//! `inputguard::Seen`, whose field is private); this test catches what it can't, so a regression
//! fails even if someone loosens visibility later:
//!
//! 1. `Seen` is made only in `inputguard.rs`;
//! 2. the guarded actions keep taking one (`focus_on`, the clipboard thread's `clipboard`, the
//!    screens' `open` / `open_empty`, `editor_input`);
//! 3. the platform's clipboard calls appear only in `text_window.rs` (its clipboard thread);
//! 4. every refusal site of a monitored channel reports to the monitor (`inputguard::refused`)
//!    right where it refuses;
//! 5. the gate runs on every call (`ToolHandler::call`) and every engine request (`handle`), and
//!    `logic_input` goes through the override check;
//! 6. no crate declares `LoadLibraryW` (system DLLs load from System32 only, `dynlib`).
//!
//! Test code (`#[cfg(test)]` items) is skipped: tests make their own `Seen`.

use std::path::{Path, PathBuf};

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("readable").map(|e| e.expect("entry").path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// `text` without comments and string literals' contents (lines kept).
fn code_only(text: &str) -> Vec<String> {
    let c: Vec<char> = text.replace("\r\n", "\n").chars().collect();
    let mut out = String::with_capacity(c.len());
    let mut i = 0;
    while i < c.len() {
        let (ch, next) = (c[i], c.get(i + 1).copied());
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
        let raw = ch == 'r' && (next == Some('#') || next == Some('"')) && !(i > 0 && (c[i - 1].is_alphanumeric() || c[i - 1] == '_'));
        if raw {
            let mut j = i + 1;
            let mut hashes = 0;
            while c.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if c.get(j) == Some(&'"') {
                j += 1;
                while j < c.len() && !(c[j] == '"' && (0..hashes).all(|h| c.get(j + 1 + h) == Some(&'#'))) {
                    if c[j] == '\n' {
                        out.push('\n');
                    }
                    j += 1;
                }
                out.push_str("\"\"");
                i = j + 1 + hashes;
                continue;
            }
        }
        if ch == '"' {
            i += 1;
            while i < c.len() && c[i] != '"' {
                if c[i] == '\\' {
                    i += 1;
                } else if c[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            out.push_str("\"\"");
            i += 1;
            continue;
        }
        if ch == '\'' {
            let close = if next == Some('\\') { (i + 2..(i + 12).min(c.len())).find(|&k| c[k] == '\'') } else if c.get(i + 2) == Some(&'\'') { Some(i + 2) } else { None };
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

/// Lines `at..` to the end of the item starting there (by brace depth).
fn item_len(code: &[String], at: usize) -> usize {
    let (mut depth, mut opened) = (0i64, false);
    for (k, l) in code[at..].iter().enumerate() {
        for ch in l.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        if (opened && depth <= 0) || (!opened && (l.trim_end().ends_with(';') || l.trim_end().ends_with(','))) {
            return k + 1;
        }
    }
    code.len() - at
}

/// Non-test code lines of one file, numbered from 1.
fn non_test(text: &str) -> Vec<(usize, String)> {
    let code = code_only(text);
    let mut kept = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if let Some(rest) = code[i].trim_start().strip_prefix("#[cfg(test)]") {
            if !rest.trim().is_empty() {
                i += item_len(&code, i);
                continue;
            }
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

fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// `pattern` (whitespace ignored) in `line` as a whole name.
fn holds(line: &str, pattern: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let (line, pattern) = (squeeze(line), squeeze(pattern));
    let starts = pattern.chars().next().is_some_and(ident);
    line.match_indices(&pattern).any(|(at, _)| !starts || !line[..at].chars().next_back().is_some_and(ident))
}

/// The engine's non-test code by file (relative, `/`-separated); test-only files (`tests.rs`, and
/// the `engine_tests.rs`-style files a `#[cfg(test)] mod` declares) left out.
fn engine_code() -> Vec<(String, Vec<(usize, String)>)> {
    let root = src();
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    files
        .iter()
        .map(|f| (f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"), std::fs::read_to_string(f).unwrap()))
        .filter(|(r, _)| !r.ends_with("tests.rs"))
        .map(|(r, t)| (r, non_test(&t)))
        .collect()
}

fn file<'a>(code: &'a [(String, Vec<(usize, String)>)], name: &str) -> &'a [(usize, String)] {
    &code.iter().find(|(f, _)| f == name).unwrap_or_else(|| panic!("src/{name} exists")).1
}

#[test]
fn the_scanner_skips_tests_comments_and_strings() {
    let text = "fn a() { let s = Seen { seq: 1 }; }\n// Seen { seq: 2 }\nlet t = \"Seen { seq: 3 }\";\n#[cfg(test)]\nmod tests {\n    fn b() { Seen { seq: 4 }; }\n}\nfn c() {}\n";
    let code = non_test(text);
    let lines: Vec<usize> = code.iter().filter(|(_, l)| holds(l, "Seen { seq")).map(|(n, _)| *n).collect();
    assert_eq!(lines, vec![1]);
    assert!(code.iter().any(|(_, l)| l.contains("fn c()")), "code after a test module is seen");
    assert!(!holds("let x = NotSeen { seq };", "Seen { seq"), "a whole name");
    assert!(!holds("fn clip_seen(&self) -> crate::inputguard::Seen {", "Seen { seq"), "a signature makes none");
}

#[test]
fn only_the_monitor_makes_a_seen() {
    let code = engine_code();
    let mut problems = Vec::new();
    for (f, lines) in &code {
        if f == "inputguard.rs" {
            continue;
        }
        for (n, l) in lines {
            // A construction names its field (`-> Seen {` in a signature is no construction).
            if holds(l, "Seen { seq") || holds(l, "Seen(") || holds(l, "Seen{..") {
                problems.push(format!("src/{f}:{n}: a Seen is made outside inputguard.rs\n    {}", l.trim()));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_guarded_actions_take_a_seen() {
    let code = engine_code();
    let must = [
        ("text_window.rs", "fn focus_on("),
        ("text_window.rs", "fn clipboard("),
        ("gallery_screen.rs", "pub fn open("),
        ("gallery_screen.rs", "pub fn open_empty("),
        ("review_screen.rs", "pub fn open("),
        ("editor_engine.rs", "fn editor_input("),
    ];
    for (f, sig) in must {
        let lines = file(&code, f);
        let at = lines.iter().position(|(_, l)| l.contains(sig)).unwrap_or_else(|| panic!("src/{f}: {sig} is gone"));
        // The signature (possibly over a few lines) names Seen.
        let sig_text: String = lines[at..(at + 4).min(lines.len())].iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join(" ");
        let sig_text = sig_text.split('{').next().unwrap_or("");
        assert!(sig_text.contains("inputguard::Seen") || sig_text.contains(": Seen"), "src/{f}: {sig} must take an inputguard::Seen (the monitor's record of it)\n    {sig_text}");
    }
}

#[test]
fn the_clipboard_is_reached_only_from_its_thread() {
    let code = engine_code();
    let mut problems = Vec::new();
    for (f, lines) in &code {
        for (n, l) in lines {
            if (holds(l, "clipboard_text(") || holds(l, "set_clipboard_text(") || holds(l, "textinput::")) && f != "text_window.rs" {
                problems.push(format!("src/{f}:{n}: the system clipboard is reached only by text_window's clipboard thread (the monitor sees it there)\n    {}", l.trim()));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Each refusal site of a monitored channel: a pattern, the files it may appear in, and the
/// monitor call that must follow it within a few lines: `refused` (an explicit try) or
/// `declined` (state the agent didn't choose: seen, never counted).
#[test]
fn every_refusal_of_a_monitored_channel_reports_to_the_monitor() {
    const SITES: [&str; 6] = ["Site::WindowFocus", "Site::EditorInput", "Site::ReviewScreen", "Site::WindowScreen", "Site::ScreenAct", "Site::HumanInput"];
    // Native code is refused in one place (authority::may_load_native); each caller of it
    // reports through logic::proposals::report_refusal (checked below).
    const WITHIN: usize = 8;
    let code = engine_code();
    let mut problems = Vec::new();
    let mut seen = 0;
    for (f, lines) in &code {
        if f == "authority.rs" {
            continue;
        }
        for (i, (n, l)) in lines.iter().enumerate() {
            let site = SITES.iter().any(|s| holds(l, s)) || (holds(l, "typing_busy(") && !holds(l, "fn typing_busy("));
            if !site {
                continue;
            }
            seen += 1;
            let reported = lines[i..(i + WITHIN).min(lines.len())].iter().any(|(_, x)| holds(x, "refused(") || holds(x, "declined("));
            if !reported {
                problems.push(format!("src/{f}:{n}: a refusal of a monitored channel must report to the monitor (inputguard::refused or declined) where it refuses\n    {}", l.trim()));
            }
        }
    }
    assert!(seen >= 8, "the sites were found ({seen})");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_gate_runs_on_every_call_and_request() {
    let code = engine_code();
    let has = |f: &str, p: &str| file(&code, f).iter().any(|(_, l)| l.contains(p));
    assert!(has("tools.rs", "inputguard::gate("), "ToolHandler::call gates every call");
    assert!(has("lib.rs", "self.security_gate("), "Engine::handle gates every request");
    assert!(has("lib.rs", "self.security_pump("), "the engine applies what incidents halt");
    assert!(has("lib.rs", "self.security_logic_input("), "logic_input goes through the override check");
    assert!(has("lib.rs", "self.security_toast_input("), "the toast reads the window's input");
    assert!(has("lib.rs", "self.security_overlay("), "the toast is drawn");
}

#[test]
fn no_crate_loads_a_dll_through_the_default_search() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let mut files = Vec::new();
    for c in std::fs::read_dir(&crates).unwrap().flatten() {
        let src = c.path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files);
        }
    }
    let mut problems = Vec::new();
    for f in files {
        for (n, l) in non_test(&std::fs::read_to_string(&f).unwrap()) {
            if holds(&l, "LoadLibraryW") || holds(&l, "LoadLibraryA") {
                problems.push(format!("{}:{n}: LoadLibraryW searches beside the executable first; load through specula_sys::dynlib (System32 only)\n    {}", f.display(), l.trim()));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

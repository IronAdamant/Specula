//! Determinism lints for module sources, run by every `logic_build` (docs/design/DETERMINISM.md,
//! docs/design/MODULES.md).
//!
//! The SDK offers only deterministic inputs, but a module is Rust: it can still call `f32::sin`
//! (the platform's maths library, different bits on msvc, gnu and other OSes), read the wall
//! clock, iterate a `HashMap` (a random order per process), start threads, or keep state in a
//! `static mut` or `thread_local!` (which also dangles after an unload). The workspace's
//! simulation crates catch these with clippy (`disallowed_methods`, `disallowed_types` in each
//! crate's `clippy.toml`); a module built by an agent may not have one, and clippy is slower than
//! the reload budget allows. So `logic_build` scans the module's `src/` itself: a small Rust
//! tokenizer (comments, strings, raw strings, char literals and lifetimes skipped) and the same
//! list of names. Hits come back as warnings among the build's diagnostics
//! (`code: "determinism::<rule>"`, file, line, column, a help note); they don't stop the build,
//! since an agent may know better, but they say what to use instead.
//!
//! It reads names, not types: `x.round()` is reported whatever `x` is (a module's own type with a
//! `round` method too). A line ending in a comment containing `determinism: allowed` (say why
//! after it) is not reported, nor is the line after a comment line (on its own) that says so.
//!
//! Only the `.rs` files under `src/` are read: a module that pulls files from elsewhere with
//! `#[path = "..."]` or `include!` has those unscanned (keep a module's code in its `src/`).
//!
//! **Bounded work** (it runs on every build): at most [`MAX_FILES`] files, [`MAX_DIRS`] folders,
//! [`MAX_FILE_BYTES`] per file and [`MAX_TOTAL_BYTES`] in all (files past the limits are skipped and
//! counted in a note), and hits past [`MAX_HITS`] are only counted. Tokenizing is linear in the
//! source: measured 2026-10-22, 40-70 ms per MB in the engine's dev build, so the 4 MiB cap bounds a
//! scan to about a quarter of a second.

use crate::diag::Diagnostic;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Files scanned at most, folders walked, bytes per file and bytes in all.
pub const MAX_FILES: usize = 512;
pub const MAX_DIRS: usize = 256;
pub const MAX_FILE_BYTES: u64 = 1 << 20;
pub const MAX_TOTAL_BYTES: u64 = 4 << 20;
/// Hits reported at most (the rest are counted in the last one's message).
pub const MAX_HITS: usize = 64;
/// The comment that marks a deliberate use.
pub const ALLOW_MARK: &str = "determinism: allowed";

/// `f32`/`f64` methods that go through the platform's maths library (or differ by target).
const PLATFORM_MATH: [&str; 33] = [
    "sin", "cos", "tan", "sin_cos", "asin", "acos", "atan", "atan2", "exp", "exp2", "exp_m1", "ln", "log", "log2", "log10", "ln_1p",
    "powf", "powi", "hypot", "cbrt", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "floor", "ceil", "round", "round_ties_even",
    "trunc", "fract", "mul_add",
];

/// One finding in a source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// 1-based, in characters (as rustc reports them).
    pub line: u32,
    pub column: u32,
    pub end_column: u32,
    /// `platform_math`, `hash_map`, `wall_clock`, `thread`, `static_state`.
    pub rule: &'static str,
    pub message: String,
    pub help: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Punct(char),
    /// A literal (number, string, char): only its presence matters.
    Lit,
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    line: u32,
    col: u32,
    len: u32,
}

/// Splits Rust source into identifiers, punctuation and literals, skipping comments. Lines
/// marked with [`ALLOW_MARK`] are returned too.
fn tokenize(src: &str) -> (Vec<Token>, BTreeSet<u32>) {
    let chars: Vec<char> = src.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut allowed = BTreeSet::new();
    let (mut i, mut line, mut col) = (0usize, 1u32, 1u32);
    // Advances over `n` characters, keeping line and column.
    let step = |i: &mut usize, line: &mut u32, col: &mut u32, n: usize| {
        for _ in 0..n {
            if *i >= chars.len() {
                return;
            }
            if chars[*i] == '\n' {
                *line += 1;
                *col = 1;
            } else {
                *col += 1;
            }
            *i += 1;
        }
    };
    let at = |i: usize, s: &str| s.chars().enumerate().all(|(k, c)| chars.get(i + k) == Some(&c));
    while i < chars.len() {
        let c = chars[i];
        let (l0, c0) = (line, col);
        if c.is_whitespace() {
            step(&mut i, &mut line, &mut col, 1);
        } else if at(i, "//") {
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                step(&mut i, &mut line, &mut col, 1);
            }
            let text: String = chars[start..i].iter().collect();
            if text.contains(ALLOW_MARK) {
                // The comment's own line; and the next when the comment stands on its own line
                // (a comment above the code), not when it trails code.
                allowed.insert(l0);
                if !out.last().is_some_and(|t| t.line == l0) {
                    allowed.insert(l0 + 1);
                }
            }
        } else if at(i, "/*") {
            let mut depth = 0;
            loop {
                if i >= chars.len() {
                    break;
                }
                if at(i, "/*") {
                    depth += 1;
                    step(&mut i, &mut line, &mut col, 2);
                } else if at(i, "*/") {
                    depth -= 1;
                    step(&mut i, &mut line, &mut col, 2);
                    if depth == 0 {
                        break;
                    }
                } else {
                    step(&mut i, &mut line, &mut col, 1);
                }
            }
        } else if let Some(n) = raw_string_len(&chars, i) {
            step(&mut i, &mut line, &mut col, n);
            out.push(Token { tok: Tok::Lit, line: l0, col: c0, len: 1 });
        } else if c == '"' || ((c == 'b' || c == 'c') && chars.get(i + 1) == Some(&'"')) {
            if c != '"' {
                step(&mut i, &mut line, &mut col, 1);
            }
            step(&mut i, &mut line, &mut col, 1);
            while i < chars.len() && chars[i] != '"' {
                let n = if chars[i] == '\\' { 2 } else { 1 };
                step(&mut i, &mut line, &mut col, n);
            }
            step(&mut i, &mut line, &mut col, 1);
            out.push(Token { tok: Tok::Lit, line: l0, col: c0, len: 1 });
        } else if c == '\'' || (c == 'b' && chars.get(i + 1) == Some(&'\'')) {
            let q = if c == 'b' { i + 1 } else { i };
            // A char literal ('x', '\n', '\u{..}') or a lifetime / label ('a).
            if chars.get(q + 1) == Some(&'\\') {
                // Past the escaped character itself (it may be a quote).
                let mut j = q + 3;
                while j < chars.len() && chars[j] != '\'' && chars[j] != '\n' {
                    j += 1;
                }
                let n = j + 1 - i;
                step(&mut i, &mut line, &mut col, n);
                out.push(Token { tok: Tok::Lit, line: l0, col: c0, len: 1 });
            } else if chars.get(q + 2) == Some(&'\'') {
                let n = q + 3 - i;
                step(&mut i, &mut line, &mut col, n);
                out.push(Token { tok: Tok::Lit, line: l0, col: c0, len: 1 });
            } else {
                let n = q + 1 - i;
                step(&mut i, &mut line, &mut col, n);
                // The lifetime's name follows as an identifier: skip it too.
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    step(&mut i, &mut line, &mut col, 1);
                }
            }
        } else if c.is_ascii_digit() {
            // Digits, `_`, a suffix, and a fraction (a `.` followed by a digit).
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || (chars[i] == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)))
            {
                step(&mut i, &mut line, &mut col, 1);
            }
            out.push(Token { tok: Tok::Lit, line: l0, col: c0, len: 1 });
        } else if c.is_alphabetic() || c == '_' {
            if at(i, "r#") {
                step(&mut i, &mut line, &mut col, 2);
            }
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                step(&mut i, &mut line, &mut col, 1);
            }
            let name: String = chars[start..i].iter().collect();
            let len = name.chars().count() as u32;
            out.push(Token { tok: Tok::Ident(name), line: l0, col: c0, len });
        } else {
            out.push(Token { tok: Tok::Punct(c), line: l0, col: c0, len: 1 });
            step(&mut i, &mut line, &mut col, 1);
        }
    }
    (out, allowed)
}

/// The length of a raw string (`r"..."`, `r#"..."#`, `br##"..."##`, `cr"..."`) starting at `i`.
fn raw_string_len(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i;
    if matches!(chars.get(j), Some('b') | Some('c')) {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if chars.get(j) != Some(&'"') {
        return None;
    }
    j += 1;
    loop {
        if j >= chars.len() {
            return Some(j - i);
        }
        if chars[j] == '"' && (0..hashes).all(|k| chars.get(j + 1 + k) == Some(&'#')) {
            return Some(j + 1 + hashes - i);
        }
        j += 1;
    }
}

/// The determinism hits in one source file (see the module docs).
pub fn scan_source(src: &str) -> Vec<Hit> {
    scan_limited(src, usize::MAX).0
}

/// What a hit says, and what to do instead.
fn describe(rule: &str, name: &str) -> (String, String) {
    let (message, help): (String, &str) = match (rule, name) {
        ("hash_map", "RandomState") => ("`RandomState` is seeded per process".into(), "use a fixed hasher"),
        ("hash_map", _) => (
            format!("`{name}` iterates in a random order that changes every run"),
            "use BTreeMap / BTreeSet, or a Vec indexed by id (or a HashMap with a fixed hasher that you never iterate)",
        ),
        ("wall_clock", _) => (
            format!("`{name}::now()` reads the wall clock"),
            "use the tick (Cx::tick) and Cx::dt: the tick number is the only clock the simulation sees",
        ),
        ("thread", _) => (
            "a thread in a module: its timing differs every run, and it may outlive an unload".into(),
            "do the work in tick(); the engine calls modules on one thread in a fixed order",
        ),
        ("static_state", _) => (
            format!("`{name}` keeps state outside the engine (lost on reload, dangling after an unload)"),
            "keep state in the world, Cx::set_state or an RNG stream: the engine owns all state",
        ),
        (_, "floor" | "ceil" | "round" | "round_ties_even" | "trunc" | "fract") => {
            return (
                format!("`{name}` on a float goes through the platform's maths library on some targets (mingw's round is wrong)"),
                format!("use dmath::{name}(x) (specula_sdk::dmath); the _f64 versions for f64"),
            )
        }
        (_, "mul_add") => ("`mul_add` is fused on some targets and a library call on others".into(), "write a * b + c"),
        (_, "powi") => ("`powi` is folded differently by LLVM on different targets".into(), "multiply, or use dmath::pow"),
        _ => {
            return (
                format!("`{name}` calls the platform's maths library: different bits on msvc, gnu and other OSes"),
                format!("use specula_sdk::dmath (dmath::{} for f32, the _f64 versions for f64)", if name == "powf" { "pow" } else { name }),
            )
        }
    };
    (message, help.to_owned())
}

/// The first `room` hits in a source file, and how many more there were (counted, not built).
pub fn scan_limited(src: &str, room: usize) -> (Vec<Hit>, usize) {
    let (toks, allowed) = tokenize(src);
    let ident = |k: usize| match toks.get(k).map(|t| &t.tok) {
        Some(Tok::Ident(s)) => Some(s.as_str()),
        _ => None,
    };
    let punct = |k: usize, c: char| toks.get(k).is_some_and(|t| t.tok == Tok::Punct(c));
    // `::` between tokens k and k+1.
    let path_sep = |k: usize| punct(k, ':') && punct(k + 1, ':');
    let (mut hits, mut more) = (Vec::new(), 0usize);
    for (k, t) in toks.iter().enumerate() {
        let Tok::Ident(name) = &t.tok else { continue };
        // (rule, the name the message is about, highlighted length)
        let found: Option<(&'static str, &str, u32)> = match name.as_str() {
            "HashMap" | "HashSet" | "RandomState" => Some(("hash_map", name, t.len)),
            "Instant" | "SystemTime" if path_sep(k + 1) && ident(k + 3) == Some("now") => Some(("wall_clock", name, t.len + 5)),
            "thread" if path_sep(k + 1) && matches!(ident(k + 3), Some("spawn") | Some("scope")) => Some(("thread", name, t.len + 7)),
            "thread_local" if punct(k + 1, '!') => Some(("static_state", "thread_local!", t.len + 1)),
            "static" if ident(k + 1) == Some("mut") => Some(("static_state", "static mut", t.len + 4)),
            m if PLATFORM_MATH.contains(&m) && punct(k + 1, '(') => {
                // `x.sin(` (a method call) or `f32::sin(` (a path call).
                let method = k > 0 && punct(k - 1, '.');
                let float_path = k >= 3 && path_sep(k - 2) && matches!(ident(k - 3), Some("f32") | Some("f64"));
                let dmath = k >= 3 && path_sep(k - 2) && ident(k - 3) == Some("dmath");
                // `log` is also a logger's method (`cx.log(..)`): only `f32::log(..)` counts.
                let method = method && m != "log";
                ((method || float_path) && !dmath).then_some(("platform_math", m, t.len))
            }
            _ => None,
        };
        let Some((rule, about, len)) = found else { continue };
        if allowed.contains(&t.line) {
            continue;
        }
        if hits.len() >= room {
            more += 1;
            continue;
        }
        let (message, help) = describe(rule, about);
        hits.push(Hit { line: t.line, column: t.col, end_column: t.col + len, rule, message, help });
    }
    (hits, more)
}

/// What a module's source reaches outside the engine's API (docs/design/GAMEPLAY_MODULES.md,
/// "What a module can reach"). Every rule refuses the build (`native_code`): this is a lint,
/// defence in depth behind the rule that only the owner or a granted lead loads a module, never
/// a sandbox (safe Rust has known soundness holes, and a lint can be raced by whoever writes the
/// files during a build).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reach {
    pub line: u32,
    pub column: u32,
    /// `native_import` (foreign functions, native libraries, inline assembly), `unsafe_code`,
    /// `os_reach` (files, processes, sockets, the environment through `std`), `macro_path` (a
    /// path segment spliced in from a macro variable, which could spell any of those).
    pub rule: &'static str,
    pub what: String,
}

/// `std` modules a module may not name: they reach the machine around the engine's rules.
const OS_MODULES: [&str; 5] = ["fs", "process", "net", "os", "env"];

/// Reaches in one source file, in order (at most `room`).
pub fn reach_scan(src: &str, room: usize) -> Vec<Reach> {
    let (toks, _) = tokenize(src);
    let ident = |k: usize| match toks.get(k).map(|t| &t.tok) {
        Some(Tok::Ident(s)) => Some(s.as_str()),
        _ => None,
    };
    let punct = |k: usize, c: char| toks.get(k).is_some_and(|t| t.tok == Tok::Punct(c));
    let lit = |k: usize| toks.get(k).is_some_and(|t| t.tok == Tok::Lit);
    let path_sep_before = |k: usize| k >= 2 && punct(k - 1, ':') && punct(k - 2, ':');
    let path_sep_after = |k: usize| punct(k + 1, ':') && punct(k + 2, ':');
    let mut out = Vec::new();
    // Inside a `use` item (its tree may rename a module: `use std::{fs as f}`).
    let mut in_use = false;
    for (k, t) in toks.iter().enumerate() {
        if out.len() >= room {
            break;
        }
        match &t.tok {
            Tok::Punct(';') => in_use = false,
            Tok::Ident(s) if s == "use" => in_use = true,
            _ => {}
        }
        // `::$seg`: a path segment from a macro variable (`std::$m::write`); `$crate` is the
        // macro's own crate.
        if t.tok == Tok::Punct('$') && path_sep_before(k) && ident(k + 1) != Some("crate") {
            out.push(Reach { line: t.line, column: t.col, rule: "macro_path", what: "a path segment from a macro variable (it could name std::fs or std::process)".into() });
            continue;
        }
        let Tok::Ident(name) = &t.tok else { continue };
        let found: Option<(&'static str, &str)> = match name.as_str() {
            // Any `extern` but `extern crate`, `extern fn` and `extern "abi" fn` (definitions and
            // pointer types) declares foreign items, however the ABI is spelled: a literal, a
            // macro variable (`extern $abi { .. }`), or nothing.
            "extern" if !(ident(k + 1) == Some("crate") || ident(k + 1) == Some("fn") || (lit(k + 1) && ident(k + 2) == Some("fn"))) => {
                Some(("native_import", "an extern block (foreign functions)"))
            }
            // `#[link(...)]`, or `link(name = ...)` inside a `cfg_attr`.
            "link" if (k >= 2 && punct(k - 1, '[') && punct(k - 2, '#')) || (punct(k + 1, '(') && matches!(ident(k + 2), Some("name") | Some("kind"))) => {
                Some(("native_import", "#[link] (a native library)"))
            }
            "link_name" | "link_ordinal" => Some(("native_import", "link_name (a native symbol)")),
            // Any mention: `asm!`, `core::arch::asm`, `use core::arch::asm as quiet`.
            "asm" | "global_asm" | "naked_asm" => Some(("native_import", "inline assembly")),
            "unsafe" => Some(("unsafe_code", "unsafe code (raw pointers can reach any function in the process)")),
            m if OS_MODULES.contains(&m) && (path_sep_before(k) || path_sep_after(k) || in_use) => Some(("os_reach", "a std module that reaches the machine (files, processes, sockets, the environment)")),
            "Command" if path_sep_after(k) && ident(k + 3) == Some("new") => Some(("os_reach", "std::process::Command (starts programs)")),
            "TcpStream" | "TcpListener" | "UdpSocket" => Some(("os_reach", "a network socket")),
            _ => None,
        };
        if let Some((rule, what)) = found {
            out.push(Reach { line: t.line, column: t.col, rule, what: what.to_owned() });
        }
    }
    out
}

/// Bounds of one build's reach scan: files and bytes read so far.
#[derive(Default)]
pub struct ScanBudget {
    files: usize,
    bytes: u64,
}

/// The first reach in `files` (every file rustc read for one crate, from its dep-info), or why
/// the files can't be checked: too many or too big is refused too (what isn't checked isn't
/// loaded).
pub fn first_reach(files: &[PathBuf], budget: &mut ScanBudget) -> Option<(PathBuf, Reach)> {
    let refuse = |file: &Path, what: String| Some((file.to_owned(), Reach { line: 0, column: 0, rule: "not_checked", what }));
    for file in files {
        budget.files += 1;
        if budget.files > MAX_FILES {
            return refuse(file, format!("more than {MAX_FILES} compiled files (too many to check)"));
        }
        let len = std::fs::metadata(file).map_or(u64::MAX, |m| m.len());
        if len > MAX_FILE_BYTES || budget.bytes + len > MAX_TOTAL_BYTES {
            return refuse(file, format!("a compiled file too big to check (over {} KiB, or {} MiB in all)", MAX_FILE_BYTES >> 10, MAX_TOTAL_BYTES >> 20));
        }
        budget.bytes += len;
        let Ok(bytes) = std::fs::read(file) else {
            return refuse(file, "a compiled file that can't be read back".into());
        };
        if let Some(r) = reach_scan(&String::from_utf8_lossy(&bytes), 1).into_iter().next() {
            return Some((file.clone(), r));
        }
    }
    None
}

/// The `.rs` files under `dir/src`, sorted (at most [`MAX_FILES`], walking at most [`MAX_DIRS`]
/// folders).
pub(crate) fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.join("src")];
    let mut walked = 0;
    while let Some(d) = stack.pop() {
        walked += 1;
        if walked > MAX_DIRS {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() && !name.starts_with('.') && name != "target" {
                stack.push(p);
            } else if ft.is_file() && name.ends_with(".rs") && out.len() < MAX_FILES {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Scans a module crate's sources and returns the hits as build diagnostics (warnings).
pub fn scan_dir(dir: &Path) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let (mut more, mut skipped, mut total) = (0usize, 0usize, 0u64);
    for file in sources(dir) {
        let len = std::fs::metadata(&file).map_or(u64::MAX, |m| m.len());
        if len > MAX_FILE_BYTES || total + len > MAX_TOTAL_BYTES {
            skipped += 1;
            continue;
        }
        total += len;
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let (hits, extra) = scan_limited(&text, MAX_HITS - out.len());
        more += extra;
        for h in hits {
            out.push(Diagnostic {
                level: "warning".into(),
                code: Some(format!("determinism::{}", h.rule)),
                message: h.message,
                file: Some(file.clone()),
                line: h.line,
                column: h.column,
                end_line: h.line,
                end_column: h.end_column,
                label: None,
                notes: vec![
                    format!("help: {}", h.help),
                    format!("note: the simulation must give the same bits on every run and machine (DETERMINISM.md); mark a deliberate use with `// {ALLOW_MARK}: <why>`"),
                ],
                fixes: Vec::new(),
                rendered: None,
                package: None,
            });
        }
        // What a module reaches outside the engine's API is refused after the build, over every
        // file rustc compiled (crate::build::check_compiled), not warned about here.
    }
    if more > 0 {
        if let Some(last) = out.last_mut() {
            last.notes.push(format!("note: {more} more determinism warning(s) not listed"));
        }
    }
    if skipped > 0 {
        // Said once, so an agent knows the scan wasn't complete.
        out.push(Diagnostic {
            level: "warning".into(),
            code: Some("determinism::not_scanned".into()),
            message: format!(
                "{skipped} source file(s) were not checked for determinism: over {} KiB each or {} MiB in all",
                MAX_FILE_BYTES >> 10,
                MAX_TOTAL_BYTES >> 20
            ),
            file: None,
            line: 0,
            column: 0,
            end_line: 0,
            end_column: 0,
            label: None,
            notes: Vec::new(),
            fixes: Vec::new(),
            rendered: None,
            package: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(src: &str) -> Vec<(&'static str, u32, u32)> {
        scan_source(src).into_iter().map(|h| (h.rule, h.line, h.column)).collect()
    }

    #[test]
    fn finds_each_kind_of_nondeterminism() {
        let src = r#"use std::collections::HashMap;
fn f(x: f32, m: &HashMap<u32, u32>) -> f32 {
    let t = std::time::Instant::now();
    std::thread::spawn(|| {});
    x.sin() + f32::cos(x) + (2.0f32).powi(2) + x.floor()
}
static mut COUNT: u32 = 0;
thread_local! { static S: u32 = 0; }
"#;
        assert_eq!(
            rules(src),
            vec![
                ("hash_map", 1, 23),
                ("hash_map", 2, 18),
                ("wall_clock", 3, 24),
                ("thread", 4, 10),
                ("platform_math", 5, 7),
                ("platform_math", 5, 20),
                ("platform_math", 5, 38),
                ("platform_math", 5, 50),
                ("static_state", 7, 1),
                ("static_state", 8, 1),
            ]
        );
    }

    #[test]
    fn comments_strings_lifetimes_dmath_and_marked_lines_are_not_reported() {
        let src = r##"// x.sin() in a comment, HashMap too
/* nested /* x.cos() */ HashMap */
fn g<'a>(s: &'a str, x: f32) -> f32 {
    let _ = "x.sin() HashMap Instant::now()";
    let _ = r#"x.round() "quoted" HashSet"#;
    let _ = b"thread::spawn";
    let _c = '"';
    let _d = '\'';
    dmath::sin(x) + specula_sdk::dmath::floor(x) + x.abs() + x.sqrt() + sin(x)
}
fn h(x: f32) -> f32 {
    x.sin() // determinism: allowed: display only
}
fn h2(x: f32) -> f32 {
    let y = x; // determinism: allowed: trailing, so only this line
    y.cos()
}
fn k(x: f32) -> f32 {
    // determinism: allowed: a cosmetic wobble
    x.cos()
}
"##;
        // Only the line after a trailing mark is reported (review: a trailing mark silenced it).
        assert_eq!(rules(src), vec![("platform_math", 16, 7)]);
    }

    /// Review: 1 MB of mark lines and hits took 0.41 s (a linear allowed-line lookup, every hit
    /// built before the cut). Marks are a set now and hits past the room are only counted.
    #[test]
    fn big_sources_are_cheap_and_hits_past_the_room_are_counted() {
        let mut s = String::new();
        while s.len() < 480_000 {
            s.push_str("//determinism: allowed\n");
        }
        let marked = s.lines().count();
        while s.len() < 1_000_000 {
            s.push_str("HashMap\n");
        }
        let (hits, more) = scan_limited(&s, 64);
        assert_eq!(hits.len(), 64);
        // The first HashMap line follows a standalone mark line, so it is allowed.
        assert_eq!(hits.len() + more, s.lines().count() - marked - 1);
        assert_eq!(hits[0].line as usize, marked + 2);
    }

    #[test]
    fn a_module_folder_is_scanned_into_warnings() {
        let dir = std::env::temp_dir().join(format!("specula-lint-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/inner")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "fn a(x: f32) -> f32 { x.sin() }\n").unwrap();
        std::fs::write(dir.join("src/inner/b.rs"), "fn b() { let _ = std::time::SystemTime::now(); }\n").unwrap();
        std::fs::write(dir.join("src/notes.txt"), "x.sin()").unwrap();
        let d = scan_dir(&dir);
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|d| d.level == "warning" && d.code.as_deref().is_some_and(|c| c.starts_with("determinism::"))));
        let j = d[0].to_json(&dir, false);
        assert_eq!(j.get("file").and_then(specula_core::Json::as_str), Some("src/inner/b.rs"));
        assert_eq!(j.get("code").and_then(specula_core::Json::as_str), Some("determinism::wall_clock"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The example modules follow the rules they are the templates for: no determinism
    /// warnings, and (in every file their libraries compile) nothing that reaches the machine.
    #[test]
    fn the_example_modules_are_clean() {
        for m in ["ambulo", "inventarium", "facultas", "colloquium", "quaestio", "exemplum"] {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games").join(m);
            let d = scan_dir(&dir);
            assert!(d.is_empty(), "{m}: {:?}", d.iter().map(|d| (&d.message, d.line)).collect::<Vec<_>>());
            // Their libraries (not their #[cfg(test)] files, which never reach the library).
            let mut lib: Vec<PathBuf> = sources(&dir).into_iter().filter(|f| !f.ends_with("tests.rs")).collect();
            lib.extend(std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")));
            assert_eq!(first_reach(&lib, &mut ScanBudget::default()), None, "{m}");
        }
    }

    /// Reaching the machine is found however it is spelled (each of the review's bypasses of the
    /// first scan, gmods R1 blocker 1); strings and comments don't count.
    #[test]
    fn native_imports_and_other_reaches_are_found() {
        let rules = |s: &str| reach_scan(s, 64).into_iter().map(|r| r.rule).collect::<Vec<_>>();
        let keylogger = r#"
            #[link(name = "user32")]
            extern "system" { fn GetAsyncKeyState(k: i32) -> i16; }
        "#;
        assert_eq!(rules(keylogger), vec!["native_import", "native_import"]);
        assert_eq!(rules("extern { fn f(); }"), vec!["native_import"]);
        assert_eq!(rules("#[cfg_attr(windows, link(name = \"x\"))] extern \"C\" {}"), vec!["native_import", "native_import"]);
        assert_eq!(rules("extern \"C\" { #[link_name = \"y\"] fn x(); }"), vec!["native_import", "native_import"]);
        assert_eq!(rules("fn f() { unsafe { core::arch::asm!(\"nop\") } }"), vec!["unsafe_code", "native_import"]);
        assert_eq!(rules("macro_rules! m { () => { extern \"system\" { fn k(); } } }"), vec!["native_import"], "inside a macro too");
        // p1: the ABI from a macro variable.
        assert_eq!(rules("macro_rules! imp { ($abi:literal) => { extern $abi { fn k(); } } } imp!(\"system\");")[0], "native_import");
        // p2: asm renamed on import.
        assert_eq!(rules("use core::arch::asm as quiet; fn f() { quiet!(\"nop\") }"), vec!["native_import"]);
        assert_eq!(rules("use core::arch::global_asm; use core::arch::naked_asm;"), vec!["native_import", "native_import"]);
        // unsafe anywhere (raw pointers walk to GetProcAddress); #![deny(unsafe_code)] is not unsafe.
        assert_eq!(rules("#![deny(unsafe_code)] fn f(p: *const u8) -> u8 { unsafe { *p } }"), vec!["unsafe_code"]);
        // An extern "C" function definition, an extern fn pointer type and extern crate are no imports.
        assert!(rules("pub extern \"C\" fn cb() {} type F = extern fn(); type G = unsafe_free; extern crate alloc; // extern \"C\" { }\nconst S: &str = \"extern {\";").is_empty());
        // std's reach: by path, renamed in a use tree, through a glob, or spliced by a macro.
        assert_eq!(rules("let c = std::process::Command::new(\"powershell\"); let s = std::net::TcpStream::connect(a);"), vec!["os_reach", "os_reach", "os_reach", "os_reach"]);
        assert_eq!(rules("use std::{fs as f}; fn w() { f::write(\"x\", \"y\"); }"), vec!["os_reach"]);
        assert_eq!(rules("use std::*; fn w() { fs::write(\"x\", \"y\"); }"), vec!["os_reach"]);
        assert_eq!(rules("fn r() -> String { std::env::var(\"HOME\").unwrap() }"), vec!["os_reach"]);
        assert_eq!(rules("macro_rules! w { ($m:ident) => { std::$m::write(\"a\", \"b\") } } w!(fs);"), vec!["macro_path"]);
        assert!(rules("macro_rules! m { () => { $crate::x() } }").is_empty(), "$crate is the macro's own crate");
        // Near misses are not reaches: a variable named link, an index, a call; Command without
        // ::new; a method named process; env! (a compile-time constant).
        assert!(rules("let link = 1; let a = v[link]; link(x); f(link, kind); let b = Command::other(); let c = x.Command; fn process(&self) {} let v = env!(\"CARGO_PKG_VERSION\");").is_empty());
        assert_eq!(rules("#[link]
fn f() {}"), vec!["native_import"], "an attribute at the very start");
        assert_eq!(reach_scan(&"unsafe {} ".repeat(100), 5).len(), 5, "bounded");
        // Compiled files (any extension: #[path = "imp.txt"], include!("imp.inc")): the first
        // reach, or files too many or too big to check.
        let dir = std::env::temp_dir().join(format!("specula-reach-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/deep")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "mod deep;\n").unwrap();
        std::fs::write(dir.join("src/deep/imp.txt"), "fn a() {}\n").unwrap();
        let files = vec![dir.join("src/lib.rs"), dir.join("src/deep/imp.txt")];
        assert_eq!(first_reach(&files, &mut ScanBudget::default()), None);
        std::fs::write(dir.join("src/deep/imp.txt"), "\n\nextern \"system\" { fn GetKeyState(k: i32) -> i16; }\n").unwrap();
        let (file, r) = first_reach(&files, &mut ScanBudget::default()).unwrap();
        assert!(file.ends_with("imp.txt") && r.line == 3 && r.rule == "native_import", "{file:?} {r:?}");
        std::fs::write(dir.join("src/deep/imp.txt"), vec![b' '; MAX_FILE_BYTES as usize + 1]).unwrap();
        assert_eq!(first_reach(&files, &mut ScanBudget::default()).unwrap().1.rule, "not_checked", "too big is refused");
        let many = vec![dir.join("src/lib.rs"); MAX_FILES + 1];
        assert_eq!(first_reach(&many, &mut ScanBudget::default()).unwrap().1.rule, "not_checked", "too many is refused");
        assert_eq!(first_reach(&[dir.join("missing.rs")], &mut ScanBudget::default()).unwrap().1.rule, "not_checked", "unreadable is refused");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

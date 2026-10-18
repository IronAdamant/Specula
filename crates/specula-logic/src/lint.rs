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
//! after it) is not reported, nor is the line after a comment line that says so.

use crate::diag::Diagnostic;
use std::path::{Path, PathBuf};

/// Files scanned at most, and bytes per file.
pub const MAX_FILES: usize = 512;
pub const MAX_FILE_BYTES: u64 = 1 << 20;
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
fn tokenize(src: &str) -> (Vec<Token>, Vec<u32>) {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut allowed = Vec::new();
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
                // The comment's own line, and the next (a comment line above the code).
                allowed.push(l0);
                allowed.push(l0 + 1);
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
    let (toks, allowed) = tokenize(src);
    let ident = |k: usize| match toks.get(k).map(|t| &t.tok) {
        Some(Tok::Ident(s)) => Some(s.as_str()),
        _ => None,
    };
    let punct = |k: usize, c: char| toks.get(k).is_some_and(|t| t.tok == Tok::Punct(c));
    // `::` between tokens k and k+1.
    let path_sep = |k: usize| punct(k, ':') && punct(k + 1, ':');
    let mut hits = Vec::new();
    for (k, t) in toks.iter().enumerate() {
        let Tok::Ident(name) = &t.tok else { continue };
        let mut hit = |rule: &'static str, message: String, help: &str, len: u32| {
            if !allowed.contains(&t.line) {
                hits.push(Hit { line: t.line, column: t.col, end_column: t.col + len, rule, message, help: help.to_owned() });
            }
        };
        match name.as_str() {
            "HashMap" | "HashSet" => hit(
                "hash_map",
                format!("`{name}` iterates in a random order that changes every run"),
                "use BTreeMap / BTreeSet, or a Vec indexed by id (or a HashMap with a fixed hasher that you never iterate)",
                t.len,
            ),
            "RandomState" => hit("hash_map", "`RandomState` is seeded per process".into(), "use a fixed hasher", t.len),
            "Instant" | "SystemTime" if path_sep(k + 1) && ident(k + 3) == Some("now") => hit(
                "wall_clock",
                format!("`{name}::now()` reads the wall clock"),
                "use the tick (Cx::tick) and Cx::dt: the tick number is the only clock the simulation sees",
                t.len + 5,
            ),
            "thread" if path_sep(k + 1) && matches!(ident(k + 3), Some("spawn") | Some("scope")) => hit(
                "thread",
                "a thread in a module: its timing differs every run, and it may outlive an unload".into(),
                "do the work in tick(); the engine calls modules on one thread in a fixed order",
                t.len + 7,
            ),
            "thread_local" if punct(k + 1, '!') => hit(
                "static_state",
                "`thread_local!` keeps state outside the engine (lost on reload, dangling after an unload)".into(),
                "keep state in the world, Cx::set_state or an RNG stream: the engine owns all state",
                t.len + 1,
            ),
            "static" if ident(k + 1) == Some("mut") => hit(
                "static_state",
                "`static mut` keeps state outside the engine (lost on reload, dangling after an unload)".into(),
                "keep state in the world, Cx::set_state or an RNG stream: the engine owns all state",
                t.len + 4,
            ),
            m if PLATFORM_MATH.contains(&m) && punct(k + 1, '(') => {
                // `x.sin(` (a method call) or `f32::sin(` (a path call).
                let method = k > 0 && punct(k - 1, '.');
                let float_path = k >= 3 && path_sep(k - 2) && matches!(ident(k - 3), Some("f32") | Some("f64"));
                let dmath = k >= 3 && path_sep(k - 2) && ident(k - 3) == Some("dmath");
                // `log` is also a logger's method (`cx.log(..)`): only `f32::log(..)` counts.
                let method = method && m != "log";
                if (method || float_path) && !dmath {
                    let (msg, help) = match m {
                        "floor" | "ceil" | "round" | "round_ties_even" | "trunc" | "fract" => (
                            format!("`{m}` on a float goes through the platform's maths library on some targets (mingw's round is wrong)"),
                            format!("use dmath::{m}(x) (specula_sdk::dmath); the _f64 versions for f64"),
                        ),
                        "mul_add" => ("`mul_add` is fused on some targets and a library call on others".into(), "write a * b + c".into()),
                        "powi" => ("`powi` is folded differently by LLVM on different targets".into(), "multiply, or use dmath::pow".into()),
                        _ => (
                            format!("`{m}` calls the platform's maths library: different bits on msvc, gnu and other OSes"),
                            format!("use specula_sdk::dmath (dmath::{} for f32, the _f64 versions for f64)", if m == "powf" { "pow" } else { m }),
                        ),
                    };
                    hit("platform_math", msg, &help, t.len);
                }
            }
            _ => {}
        }
    }
    hits
}

/// The `.rs` files under `dir/src`, sorted (at most [`MAX_FILES`]).
fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.join("src")];
    while let Some(d) = stack.pop() {
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
    let mut more = 0usize;
    for file in sources(dir) {
        if std::fs::metadata(&file).map_or(true, |m| m.len() > MAX_FILE_BYTES) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        for h in scan_source(&text) {
            if out.len() == MAX_HITS {
                more += 1;
                continue;
            }
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
    }
    if more > 0 {
        if let Some(last) = out.last_mut() {
            last.notes.push(format!("note: {more} more determinism warning(s) not listed"));
        }
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
fn k(x: f32) -> f32 {
    // determinism: allowed: a cosmetic wobble
    x.cos()
}
"##;
        assert_eq!(rules(src), vec![]);
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

    /// The example module follows the rules it is the template for.
    #[test]
    fn the_example_module_is_clean() {
        let ambulo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/ambulo");
        let d = scan_dir(&ambulo);
        assert!(d.is_empty(), "{:?}", d.iter().map(|d| (&d.message, d.line)).collect::<Vec<_>>());
    }
}

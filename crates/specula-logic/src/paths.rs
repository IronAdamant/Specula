//! Paths a module's source names at compile time: `#[path = "..."]` on a `mod` (also through
//! `cfg_attr`), and the `include_str!` / `include_bytes!` / `include!` macros.
//!
//! Two checks, for two jobs:
//!
//! - **The authoritative one runs after the build** ([`check_dep_info`]): rustc's dep-info for the
//!   module crate lists every file the compiler read, however the source spelled it (a renamed
//!   macro, a `macro_rules!` wrapper, escapes, a path split over lines). Every one must lie inside
//!   the module's own folder (never its `target/`), or the build is refused and its library never
//!   loads.
//! - **A token scan** ([`check_source`]) gives the early hint at commit and before a build: it
//!   reads the source as Rust tokens (comments and strings understood, escapes decoded,
//!   `cfg_attr` and inline `mod` nesting followed) and refuses what plainly reaches out. Macro
//!   indirection it can't see is the dep-info check's to catch.
//!
//! This is about determinism and coverage, not a sandbox: a module is native code running in the
//! engine and can read files at run time anyway. What the check guarantees is that no file
//! outside the module (engine sources, host files) is compiled or embedded into it, so every line
//! a module runs was scanned by the determinism lints and lives in the module's folder.

use std::path::{Path, PathBuf};

/// One refused path: 1-based line and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathProblem {
    pub line: usize,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    /// A string literal's decoded value (`None`: a byte string or char we don't need).
    Str(String),
    Punct(char),
}

/// Rust tokens with their 1-based lines: comments dropped, string literals decoded.
fn tokens(text: &str) -> Vec<(Tok, usize)> {
    let c: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line) = (0usize, 1usize);
    let at = |i: usize| c.get(i).copied().unwrap_or('\0');
    while i < c.len() {
        let ch = c[i];
        if ch == '\n' {
            line += 1;
            i += 1;
        } else if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && at(i + 1) == '/' {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && at(i + 1) == '*' {
            let mut depth = 0;
            while i < c.len() {
                if c[i] == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if c[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            let word: String = c[start..i].iter().collect();
            // String prefixes: b"..", r"..", r#".."#, br"..", c"..".
            let raw = matches!(word.as_str(), "r" | "br" | "cr") && (at(i) == '"' || at(i) == '#');
            if raw {
                let hashes = c[i..].iter().take_while(|x| **x == '#').count();
                i += hashes;
                if at(i) != '"' {
                    out.push((Tok::Ident(word), line));
                    continue;
                }
                i += 1;
                let l0 = line;
                let mut s = String::new();
                while i < c.len() {
                    if c[i] == '"' && c[i + 1..].iter().take(hashes).filter(|x| **x == '#').count() == hashes && c.len() >= i + 1 + hashes {
                        i += 1 + hashes;
                        break;
                    }
                    if c[i] == '\n' {
                        line += 1;
                    }
                    s.push(c[i]);
                    i += 1;
                }
                out.push((Tok::Str(s), l0));
            } else if matches!(word.as_str(), "b" | "c") && at(i) == '"' {
                let l0 = line;
                let (s, ni, nl) = string(&c, i + 1, line);
                i = ni;
                line = nl;
                out.push((Tok::Str(s), l0));
            } else {
                out.push((Tok::Ident(word), line));
            }
        } else if ch == '"' {
            let l0 = line;
            let (s, ni, nl) = string(&c, i + 1, line);
            i = ni;
            line = nl;
            out.push((Tok::Str(s), l0));
        } else if ch == '\'' {
            // A char literal ('a', '\n', '\u{..}') or a lifetime ('a).
            if at(i + 1) == '\\' {
                i += 2;
                while i < c.len() && c[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if at(i + 2) == '\'' {
                i += 3;
            } else {
                out.push((Tok::Punct('\''), line));
                i += 1;
            }
        } else {
            out.push((Tok::Punct(ch), line));
            i += 1;
        }
    }
    out
}

/// A string literal's body from `i` (after the opening quote): decoded value, next index, line.
fn string(c: &[char], mut i: usize, mut line: usize) -> (String, usize, usize) {
    let mut s = String::new();
    while i < c.len() {
        match c[i] {
            '"' => return (s, i + 1, line),
            '\\' => {
                let e = c.get(i + 1).copied().unwrap_or('\0');
                i += 2;
                match e {
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    'r' => s.push('\r'),
                    '0' => s.push('\0'),
                    'x' => {
                        let h: String = c[i.min(c.len())..(i + 2).min(c.len())].iter().collect();
                        if let Ok(v) = u8::from_str_radix(&h, 16) {
                            s.push(v as char);
                        }
                        i += 2;
                    }
                    'u' => {
                        let end = c[i..].iter().position(|x| *x == '}').map_or(c.len(), |p| i + p);
                        let h: String = c[(i + 1).min(end)..end].iter().collect();
                        if let Some(ch) = u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) {
                            s.push(ch);
                        }
                        i = end + 1;
                    }
                    '\n' => {
                        // Line continuation: the newline and the next line's leading space go.
                        line += 1;
                        while i < c.len() && c[i].is_whitespace() {
                            if c[i] == '\n' {
                                line += 1;
                            }
                            i += 1;
                        }
                    }
                    other => s.push(other),
                }
            }
            ch => {
                if ch == '\n' {
                    line += 1;
                }
                s.push(ch);
                i += 1;
            }
        }
    }
    (s, i, line)
}

/// Where `target` lands, written in the module file `file` (inside the module, like `src/lib.rs`)
/// with `base` the folder relative paths start from; `None` when it is absolute or climbs out.
fn resolve(base: &[String], target: &str) -> Option<String> {
    let t = target.replace('\\', "/");
    if t.starts_with('/') || t.contains(':') || t.starts_with("//") {
        return None;
    }
    let mut parts: Vec<String> = base.to_vec();
    for seg in t.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s.to_owned()),
        }
    }
    Some(parts.join("/"))
}

/// Inside the module, and not its build output.
fn allowed(rel: &str) -> bool {
    let first = rel.split('/').next().unwrap_or("").to_ascii_lowercase();
    !rel.is_empty() && first != "target" && first != ".cargo"
}

const MACROS: [&str; 3] = ["include_str", "include_bytes", "include"];

/// Problems with the compile-time paths in `text`, the module file at `rel_file` (relative to the
/// module, `/`-separated, e.g. `src/lib.rs`). An early hint: see the module docs.
pub fn check_source(rel_file: &str, text: &str) -> Vec<PathProblem> {
    let toks = tokens(text);
    let mut out = Vec::new();
    let mut file_dir: Vec<String> = rel_file.split('/').map(str::to_owned).collect();
    file_dir.pop();
    // Inline `mod name { ... }` nesting, as (name, brace depth inside it).
    let mut mods: Vec<(String, usize)> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < toks.len() {
        let (t, line) = &toks[i];
        match t {
            Tok::Punct('{') => {
                depth += 1;
                if i >= 2 {
                    if let (Tok::Ident(m), Tok::Ident(name)) = (&toks[i - 2].0, &toks[i - 1].0) {
                        if m == "mod" {
                            mods.push((name.clone(), depth));
                        }
                    }
                }
            }
            Tok::Punct('}') => {
                if mods.last().is_some_and(|(_, d)| *d == depth) {
                    mods.pop();
                }
                depth = depth.saturating_sub(1);
            }
            Tok::Ident(name) if MACROS.contains(&name.as_str()) && matches!(toks.get(i + 1), Some((Tok::Punct('!'), _))) => {
                let m = format!("{name}!");
                match (toks.get(i + 2), toks.get(i + 3)) {
                    (Some((Tok::Punct('(' | '[' | '{'), _)), Some((Tok::Str(p), _))) => {
                        // Includes resolve against the file's own folder.
                        if !resolve(&file_dir, p).is_some_and(|r| allowed(&r)) {
                            out.push(PathProblem { line: *line, message: format!("{m}(\"{p}\") reaches outside the module's folder: modules may only include their own files") });
                        }
                    }
                    (Some((Tok::Punct('(' | '[' | '{'), _)), _) => {
                        out.push(PathProblem { line: *line, message: format!("{m} takes a plain string literal in a module (a path built with concat!/env! could reach anywhere)") });
                    }
                    _ => {}
                }
            }
            Tok::Punct('#') => {
                // An attribute: #[...] or #![...], `path = "..."` anywhere in it (cfg_attr too).
                let mut j = i + 1;
                if matches!(toks.get(j), Some((Tok::Punct('!'), _))) {
                    j += 1;
                }
                if matches!(toks.get(j), Some((Tok::Punct('['), _))) {
                    let mut level = 0;
                    let mut k = j;
                    while k < toks.len() {
                        match &toks[k].0 {
                            Tok::Punct('[') => level += 1,
                            Tok::Punct(']') => {
                                level -= 1;
                                if level == 0 {
                                    break;
                                }
                            }
                            Tok::Ident(p) if p == "path" && matches!(toks.get(k + 1), Some((Tok::Punct('='), _))) => match toks.get(k + 2) {
                                Some((Tok::Str(target), l)) => {
                                    // `#[path]` inside inline modules starts from their folder.
                                    let mut base = file_dir.clone();
                                    base.extend(mods.iter().map(|(n, _)| n.clone()));
                                    if !resolve(&base, target).is_some_and(|r| allowed(&r)) {
                                        out.push(PathProblem { line: *l, message: format!("#[path = \"{target}\"] reaches outside the module's folder: a module may only compile its own files") });
                                    }
                                }
                                _ => out.push(PathProblem { line: toks[k].1, message: "#[path] takes a plain string literal".into() }),
                            },
                            _ => {}
                        }
                        k += 1;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// [`check_source`] over every `.rs` file under the module's `src/`: the first problem as
/// `(code, message)`. An early hint before Cargo runs; [`check_dep_info`] is the real check.
pub fn check_module(dir: &Path) -> Result<(), (&'static str, String)> {
    for file in crate::lint::sources(dir) {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let rel = file.strip_prefix(dir).unwrap_or(&file).to_string_lossy().replace('\\', "/");
        if let Some(p) = check_source(&rel, &text).into_iter().next() {
            return Err(("source_path", format!("{rel}:{}: {}", p.line, p.message)));
        }
    }
    Ok(())
}

/// The files a make-style dep-info file (`target: dep dep ...`) lists for its first target.
pub fn dep_info_files(text: &str) -> Vec<String> {
    let first: String = {
        // Joined continuation lines, up to the first blank line.
        let mut s = String::new();
        for l in text.lines() {
            if l.trim().is_empty() {
                break;
            }
            match l.strip_suffix('\\') {
                Some(head) if !head.ends_with('\\') => {
                    s.push_str(head);
                    s.push(' ');
                }
                _ => {
                    s.push_str(l);
                    break;
                }
            }
        }
        s
    };
    // The target ends at the first `: ` (Windows paths have `C:` inside them).
    let Some(colon) = first.find(": ").or_else(|| first.strip_suffix(':').map(|s| s.len())) else { return Vec::new() };
    let deps = &first[(colon + 1).min(first.len())..];
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = deps.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars.peek() == Some(&' ') => {
                cur.push(' ');
                chars.next();
            }
            ' ' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The authoritative check, after a build: every file rustc read for the module crate (its
/// dep-info `deps_file`) lies inside the module folder `dir`, never in its `target/`. Relative
/// paths are taken from `root` (the Cargo workspace root, where Cargo runs rustc from).
pub fn check_dep_info(deps_file: &Path, dir: &Path, root: &Path) -> Result<(), (&'static str, String)> {
    let text = std::fs::read_to_string(deps_file).map_err(|e| ("source_path", format!("can't read the build's dep-info {}: {e}; the module isn't loaded", deps_file.display())))?;
    let dir = crate::fix::canonical(dir);
    let files = dep_info_files(&text);
    if files.is_empty() {
        return Err(("source_path", format!("the build's dep-info {} lists no files; the module isn't loaded", deps_file.display())));
    }
    for f in files {
        let p = PathBuf::from(&f);
        let p = if p.is_absolute() { p } else { root.join(p) };
        let c = crate::fix::canonical(&p);
        let rel = c.strip_prefix(&dir).ok().map(|r| r.to_string_lossy().replace('\\', "/"));
        if !rel.as_deref().is_some_and(allowed) {
            // Names only the path, never the file's contents.
            return Err(("source_path", format!("the module's build read {}, outside the module's folder: modules may only compile and include their own files (refused; the library isn't loaded)", c.display())));
        }
    }
    Ok(())
}

/// The newest rustc dep-info for library target `name` in a Cargo target folder's `deps`
/// (`<name>-<hash>.d`, or `<name>.d` for a cdylib), from one of the build's artifacts.
pub fn find_dep_info(artifact: &Path, name: &str) -> Option<PathBuf> {
    let parent = artifact.parent()?;
    let deps = if parent.file_name().is_some_and(|n| n == "deps") { parent.to_path_buf() } else { parent.join("deps") };
    let stem = name.replace('-', "_");
    let prefix = format!("{stem}-");
    std::fs::read_dir(&deps)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            // cdylibs carry no hash in their names (`ambulo.d`); rlibs and metadata do.
            (n.starts_with(&prefix) && n.ends_with(".d")) || n == format!("{stem}.d")
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_time_paths_stay_in_the_module() {
        let ok = "#[path = \"inner/x.rs\"]\nmod x;\nconst A: &str = include_str!(\"data.txt\");\nconst B: &[u8] = include_bytes!(r\"assets/b.bin\");\n// include_str!(\"../../x\") in a comment\nfn my_include() {}\n";
        assert!(check_source("src/lib.rs", ok).is_empty(), "{:?}", check_source("src/lib.rs", ok));
        assert!(check_source("src/a/b.rs", "include!(\"../c.rs\");").is_empty(), "src/c.rs is inside");
        assert!(check_source("src/lib.rs", "actions: include_str!(\"../actions.json\"),").is_empty(), "ambulo's own top-level file");
    }

    /// The review's 18 cases (round 3): what the token scan sees directly, and what only the
    /// build's dep-info can (macro indirection).
    #[test]
    fn the_reviewers_evasions() {
        let bs = '\\';
        let refused = |src: &str| !check_source("src/lib.rs", src).is_empty();
        let direct = [
            ("E1 space before !", "const W: &str = include_str !(\"C:/Windows/win.ini\");".to_owned()),
            ("E2 // in an earlier string", "const S: &str = \"//\"; const W: &str = include_str!(\"C:/Windows/win.ini\");".to_owned()),
            ("E3 x escapes", format!("const W: &str = include_str!(\"{bs}x43{bs}x3a/Windows/win.ini\");")),
            ("E4 u escapes", format!("const W: &str = include_str!(\"{bs}u{{43}}{bs}u{{3a}}/Windows/win.ini\");")),
            ("E6 cfg_attr path", "#[cfg_attr(all(), path = \"C:/Windows/x.rs\")]\nmod e;".to_owned()),
            ("E7 hash space bracket", "# [path = \"C:/Windows/x.rs\"]\nmod e;".to_owned()),
            ("E8 args on next line", "const W: &str = include_str!\n(\"C:/Windows/win.ini\");".to_owned()),
            ("E9 = on next line", "#[path\n= \"C:/Windows/x.rs\"]\nmod e;".to_owned()),
            ("E11 x2e climbs out", format!("const W: &str = include_str!(\"{bs}x2e{bs}x2e/{bs}x2e{bs}x2e/{bs}x2e{bs}x2e/Cargo.toml\");")),
            ("E12 TARGET case", "const W: &[u8] = include_bytes!(\"../TARGET/debug/x.dll\");".to_owned()),
            ("E14 raw string", "const W: &str = include_str!(r#\"C:/Windows/win.ini\"#);".to_owned()),
            ("E15 verbatim", format!("const W: &str = include_str!(\"{bs}{bs}{bs}{bs}?{bs}{bs}C:{bs}{bs}Windows{bs}{bs}win.ini\");")),
            ("E17 block comment quote", "/* \" */ const W: &str = include_str!(\"C:/Windows/win.ini\");".to_owned()),
            ("E18 doc attr", "#[doc = include_str!(\"C:/Windows/win.ini\")]\npub fn f() {}".to_owned()),
        ];
        for (name, src) in &direct {
            assert!(refused(src), "{name} should be refused: {src}");
        }
        // Macro indirection hides the path from any token scan; the build's dep-info catches it
        // (the live check in scripts/project_check.py builds these).
        let indirect = [
            "use core::include_str as grab;\nconst W: &str = grab!(\"C:/Windows/win.ini\");",
            "macro_rules! m { ($i:ident) => { $i!(\"C:/Windows/win.ini\") } }\nconst W: &str = m!(include_str);",
        ];
        for src in indirect {
            let _ = refused(src);
        }
        // E13: a literal wrapper is a non-literal include in the scan's eyes: refused.
        assert!(refused("macro_rules! inc { ($p:literal) => { include_str!($p) } }\nconst W: &str = inc!(\"C:/Windows/win.ini\");"));
        // E16 was a false refusal: inside `mod a { }` a #[path] starts from src/a/.
        assert!(!refused("mod a {\n#[path = \"../../x.rs\"]\nmod b;\n}"), "src/a/../../x.rs is x.rs, inside the module");
        assert!(refused("mod a {\n#[path = \"../../../x.rs\"]\nmod b;\n}"));
        // Strings and lifetimes don't confuse the scanner.
        assert!(!refused("fn f<'a>(x: &'a str) -> char { let _ = \"include_str!(\\\"/etc/passwd\\\")\"; 'x' }"));
    }

    #[test]
    fn dep_info_is_parsed_and_checked() {
        let d = "C:\\w\\target\\debug\\deps\\ambulo-1.d: mods\\ambulo\\src\\lib.rs mods\\ambulo\\src\\with\\ space.rs mods\\ambulo\\actions.json\n\nmods\\ambulo\\src\\lib.rs:\n";
        assert_eq!(dep_info_files(d), ["mods\\ambulo\\src\\lib.rs", "mods\\ambulo\\src\\with space.rs", "mods\\ambulo\\actions.json"]);
        let root = std::env::temp_dir().join(format!("specula-depinfo-{}", std::process::id()));
        let dir = root.join("mods").join("ambulo");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("target").join("debug")).unwrap();
        for f in ["src/lib.rs", "actions.json", "target/debug/y.txt"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        let good = root.join("good.d");
        std::fs::write(&good, "x.dll: mods/ambulo/src/lib.rs mods/ambulo/actions.json\n").unwrap();
        assert!(check_dep_info(&good, &dir, &root).is_ok());
        let bad = root.join("bad.d");
        std::fs::write(&bad, "x.dll: mods/ambulo/src/lib.rs C:/Windows/win.ini\n").unwrap();
        let e = check_dep_info(&bad, &dir, &root).unwrap_err();
        assert_eq!(e.0, "source_path");
        assert!(e.1.contains("win.ini"));
        let tgt = root.join("tgt.d");
        std::fs::write(&tgt, "x.dll: mods/ambulo/src/lib.rs mods/ambulo/target/debug/y.txt\n").unwrap();
        assert!(check_dep_info(&tgt, &dir, &root).is_err(), "never the build output");
        assert!(check_dep_info(&root.join("missing.d"), &dir, &root).is_err(), "fails closed");
        let _ = std::fs::remove_dir_all(&root);
    }
}

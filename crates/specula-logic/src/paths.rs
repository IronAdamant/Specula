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

/// The rustc dep-info of the build's artifact for library target `name`: the `.d` file rustc
/// wrote beside the file it compiled, found by where that file is and never by a date. Empty
/// when there is none; more than one only as said below, and then the caller checks each.
///
/// Cargo has two layouts of its target folder, and a folder two toolchains have built into holds
/// both (Rust 1.100 moved `deps/` to `build/<package>/<hash>/out/`):
/// - An artifact still where rustc wrote it (`deps/libx-<hash>.rlib`; from 1.100
///   `build/x/<hash>/out/libx-<hash>.rlib`) has its dep-info in its own folder.
/// - An artifact Cargo copied up into the profile folder (`debug/ambulo.dll`) has it beside the
///   build's own copy, in `deps/` or in one of `build/*/*/out/`: the copy with the artifact's
///   bytes. An older build's copy left in the folder (another toolchain's, another version of the
///   package) has other bytes, and its dep-info lists what that build read: taking it would
///   check files that are not the ones compiled. Should two builds have left copies with the
///   same bytes (review R1: nobody has made one), none is preferred: the dep-info of each is
///   returned, and the library is loaded only if every one passes.
///
/// Never the `.d` beside a copied-up artifact (Cargo's own: it lists every path dependency's
/// sources), and never a `cargo check` or `cargo clippy` metadata file sharing a folder (it
/// lists `clippy.toml` and `Cargo.toml`, and made logic_build refuse a clean module). No copy
/// with the artifact's bytes, a copy with no dep-info beside it, or more folders than the search
/// reads, is an empty answer: the build is refused.
pub fn find_dep_infos(artifact: &Path, name: &str) -> Vec<PathBuf> {
    let (Some(parent), Some(file), Some(own)) = (artifact.parent(), artifact.file_name(), library_stem(artifact, name)) else { return Vec::new() };
    let beside = |folder: &Path| Some(folder.join(format!("{own}.d"))).filter(|d| d.is_file());
    if parent.file_name().is_some_and(|n| n == "deps" || n == "out") {
        return beside(parent).into_iter().collect();
    }
    let Some(folders) = rustc_folders(parent, RUSTC_FOLDERS) else { return Vec::new() };
    let mut found = Vec::new();
    for folder in folders {
        if same_bytes(artifact, &folder.join(file)) {
            match beside(&folder) {
                Some(d) => found.push(d),
                None => return Vec::new(),
            }
        }
    }
    found
}

/// The name rustc's dep-info for `artifact` has, less `.d`, when the file is named for library
/// target `name`: `ambulo` for `ambulo.dll` and `libambulo.rlib`, `x-<hash>` for
/// `libx-<hash>.rlib` (a package name's dash is the library's underscore). `None` for a file
/// named for something else (`libambulo.dll.a`, another crate's).
fn library_stem(artifact: &Path, name: &str) -> Option<String> {
    let stem = name.replace('-', "_");
    let own = artifact.file_stem()?.to_string_lossy().into_owned();
    let own = own.strip_prefix("lib").filter(|o| o.starts_with(&stem)).map(str::to_owned).unwrap_or(own);
    (own == stem || own.starts_with(&format!("{stem}-"))).then_some(own)
}

/// The dep-info of one target's build, from every file Cargo names for it (`artifacts`): each
/// library among them (`.dll`, `.so`, `.dylib`, `.rlib` or `.rmeta`, named for `name`) is looked
/// up with [`find_dep_infos`] and the answers are joined. A library with no answer empties the
/// whole: no other file of the build stands in for it (review R2: the first answer found was
/// taken, so a module's `.rlib` could answer for the `.dll` that is loaded). Empty too when
/// none of the files is a library.
pub fn dep_infos_of(artifacts: &[PathBuf], name: &str) -> Vec<PathBuf> {
    const LIBRARIES: [&str; 5] = ["dll", "so", "dylib", "rlib", "rmeta"];
    let mut all = Vec::new();
    for a in artifacts {
        if !a.extension().is_some_and(|e| LIBRARIES.iter().any(|l| e == *l)) || library_stem(a, name).is_none() {
            continue;
        }
        let found = find_dep_infos(a, name);
        if found.is_empty() {
            return Vec::new();
        }
        all.extend(found);
    }
    all.sort();
    all.dedup();
    all
}

/// The most folders [`find_dep_infos`] looks into for a copied-up artifact's own copy. A
/// module's target holds a handful; a workspace's a few hundred.
const RUSTC_FOLDERS: usize = 4096;

/// The folders of a profile folder that rustc writes artifacts into: `deps` (before Rust 1.100)
/// and every `build/<package>/<hash>/out` (from it), in name order. `None` when there are more
/// than `most` to read: a search that stopped part-way could miss the copy it is for, so there
/// is no answer and the build is refused.
fn rustc_folders(profile: &Path, most: usize) -> Option<Vec<PathBuf>> {
    let folders = |p: &Path| -> Option<Vec<PathBuf>> {
        let entries: Vec<PathBuf> = std::fs::read_dir(p).into_iter().flatten().flatten().take(most + 1).map(|e| e.path()).collect();
        if entries.len() > most {
            return None;
        }
        let mut v: Vec<PathBuf> = entries.into_iter().filter(|p| p.is_dir()).collect();
        v.sort();
        Some(v)
    };
    let mut out = vec![profile.join("deps")];
    for package in folders(&profile.join("build"))? {
        for hash in folders(&package)? {
            if out.len() >= most {
                return None;
            }
            out.push(hash.join("out"));
        }
    }
    Some(out)
}

/// Whether two files hold the same bytes (their lengths first, then read in pieces).
fn same_bytes(a: &Path, b: &Path) -> bool {
    use std::io::Read;
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else { return false };
    if !ma.is_file() || !mb.is_file() || ma.len() != mb.len() {
        return false;
    }
    let (Ok(mut fa), Ok(mut fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else { return false };
    let (mut ba, mut bb) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let Ok(n) = fa.read(&mut ba) else { return false };
        if n == 0 {
            return true;
        }
        if fb.read_exact(&mut bb[..n]).is_err() || ba[..n] != bb[..n] {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    /// Pre-existing bug found by the write-policy lane: after `cargo clippy` in the same target
    /// folder, the newest `ambulo-<hash>.d` was clippy's (listing clippy.toml and Cargo.toml),
    /// and logic_build refused a clean module. The artifact's own dep-info is used: the one
    /// beside the build's own copy of it. Never metadata's, never Cargo's beside the copied-up
    /// file, and never another build's for being the newest.
    #[test]
    fn the_artifacts_own_dep_info_wins_never_clippys() {
        let root = std::env::temp_dir().join(format!("specula-depinfo-own-{}", std::process::id()));
        let deps = root.join("debug").join("deps");
        std::fs::create_dir_all(&deps).unwrap();
        let own = "D:/t/debug/deps/ambulo.d: games/ambulo/src/lib.rs\nD:/t/debug/deps/ambulo.dll: games/ambulo/src/lib.rs\n";
        let clippy = "D:/t/debug/deps/ambulo-5b27.d: games/ambulo/clippy.toml games/ambulo/src/lib.rs Cargo.toml\nD:/t/debug/deps/libambulo-5b27.rmeta: games/ambulo/clippy.toml Cargo.toml\n# env-dep:CLIPPY_ARGS=-D\n";
        let rlib = "D:/t/debug/deps/ambulo-77aa.d: games/ambulo/src/lib.rs\nD:/t/debug/deps/libambulo-77aa.rlib: games/ambulo/src/lib.rs\n";
        let cdylib = root.join("debug").join("ambulo.dll");
        std::fs::write(&cdylib, b"the library").unwrap();
        std::fs::write(deps.join("ambulo.dll"), b"the library").unwrap();
        // Cargo's own, beside the copied-up library: every path dependency's sources.
        std::fs::write(root.join("debug").join("ambulo.d"), "D:/t/debug/ambulo.dll: D:/engine/crates/specula-sdk/src/lib.rs games/ambulo/src/lib.rs\n").unwrap();
        std::fs::write(deps.join("ambulo.d"), own).unwrap();
        std::fs::write(deps.join("ambulo-77aa.d"), rlib).unwrap();
        // Written last: the newest file.
        std::fs::write(deps.join("ambulo-5b27.d"), clippy).unwrap();
        let none: Vec<PathBuf> = Vec::new();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), [deps.join("ambulo.d")], "the cdylib's own");
        assert_eq!(find_dep_infos(&deps.join("libambulo-77aa.rlib"), "ambulo"), [deps.join("ambulo-77aa.d")], "the rlib's own");
        assert_eq!(find_dep_infos(&cdylib, "other"), none, "a library of another name is not this crate's");
        std::fs::remove_file(deps.join("ambulo.d")).unwrap();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), none, "neither clippy's metadata, nor Cargo's own, nor a real build's for being the newest");
        // The build's copy has other bytes: what is in the profile folder is not what this
        // dep-info describes.
        std::fs::write(deps.join("ambulo.d"), own).unwrap();
        std::fs::write(deps.join("ambulo.dll"), b"another one").unwrap();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), none, "a copy with other bytes is another build's");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Rust 1.100 moved what rustc writes from `deps/` to `build/<package>/<hash>/out/` (found
    /// by CI on the push that ended M5: this machine's Rust was one release older, and with the
    /// new one every module build was refused for having "no dep-info"). Both layouts are read,
    /// and in a folder that holds both, or two builds of one package, the dep-info taken is the
    /// one beside the copy with the library's bytes.
    #[test]
    fn dep_info_is_found_in_both_layouts_of_a_target_folder() {
        let root = std::env::temp_dir().join(format!("specula-depinfo-layouts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let profile = root.join("debug");
        let one = profile.join("build/ambulo/1111111111111111/out");
        let two = profile.join("build/ambulo/2222222222222222/out");
        let sdk = profile.join("build/specula-sdk/aaaaaaaaaaaaaaaa/out");
        for d in [&one, &two, &sdk] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(one.join("ambulo.dll"), b"version one").unwrap();
        std::fs::write(one.join("ambulo.d"), "x/out/ambulo.dll: games/ambulo/src/lib.rs games/ambulo/src/one.rs\n").unwrap();
        // Written after: the newest.
        std::fs::write(two.join("ambulo.dll"), b"version two").unwrap();
        std::fs::write(two.join("ambulo.d"), "x/out/ambulo.dll: games/ambulo/src/lib.rs\n").unwrap();
        std::fs::write(profile.join("ambulo.d"), "x/debug/ambulo.dll: D:/engine/crates/specula-sdk/src/lib.rs\n").unwrap();
        let cdylib = profile.join("ambulo.dll");
        let none: Vec<PathBuf> = Vec::new();
        std::fs::write(&cdylib, b"version one").unwrap();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), [one.join("ambulo.d")], "the build whose library this is, not the newest");
        std::fs::write(&cdylib, b"version two").unwrap();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), [two.join("ambulo.d")]);
        // A dependency stays where rustc wrote it, with its dep-info beside it; a package name's
        // dash is the library's underscore.
        std::fs::write(sdk.join("libspecula_sdk-aaaaaaaaaaaaaaaa.rlib"), b"sdk").unwrap();
        std::fs::write(sdk.join("specula_sdk-aaaaaaaaaaaaaaaa.d"), "x: crates/specula-sdk/src/lib.rs\n").unwrap();
        assert_eq!(find_dep_infos(&sdk.join("libspecula_sdk-aaaaaaaaaaaaaaaa.rlib"), "specula-sdk"), [sdk.join("specula_sdk-aaaaaaaaaaaaaaaa.d")]);
        assert_eq!(find_dep_infos(&sdk.join("libspecula_sdk-aaaaaaaaaaaaaaaa.rmeta"), "specula_sdk"), [sdk.join("specula_sdk-aaaaaaaaaaaaaaaa.d")]);
        // The folder an older toolchain built into: its library and dep-info are still in deps/.
        let deps = profile.join("deps");
        std::fs::create_dir_all(&deps).unwrap();
        std::fs::write(deps.join("ambulo.dll"), b"from the older toolchain").unwrap();
        std::fs::write(deps.join("ambulo.d"), "x/deps/ambulo.dll: games/ambulo/src/lib.rs games/ambulo/src/old.rs\n").unwrap();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), [two.join("ambulo.d")], "the older toolchain's dep-info describes another library");
        std::fs::write(&cdylib, b"from the older toolchain").unwrap();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), [deps.join("ambulo.d")], "and is taken for its own");
        // A library no build's folder holds: nothing describes it.
        std::fs::write(&cdylib, b"put there by hand").unwrap();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), none);
        // Review R1: two builds whose copies have the library's bytes. Neither is preferred
        // (the first in name order was taken, which could be the one that did not make this
        // library): both are returned, for the caller to check both.
        std::fs::write(&cdylib, b"version one").unwrap();
        std::fs::write(two.join("ambulo.dll"), b"version one").unwrap();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), [one.join("ambulo.d"), two.join("ambulo.d")]);
        // A copy with the library's bytes and no dep-info: that build is not described, so
        // nothing is answered, whatever the other copy has.
        std::fs::remove_file(one.join("ambulo.d")).unwrap();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), none);
        std::fs::write(two.join("ambulo.dll"), b"version two").unwrap();
        assert_eq!(find_dep_infos(&cdylib, "ambulo"), none, "the copy is there but its dep-info is not");
        // Review R2: a build's files together. The library and its rlib share one dep-info;
        // an import library is no library of the crate; and when the library that is loaded
        // has no answer, the rlib's does not stand in for it.
        std::fs::write(one.join("ambulo.d"), "x/out/ambulo.dll: games/ambulo/src/lib.rs\n").unwrap();
        std::fs::write(&cdylib, b"version one").unwrap();
        let (rlib, import) = (profile.join("libambulo.rlib"), profile.join("libambulo.dll.a"));
        for f in [&rlib, &one.join("libambulo.rlib")] {
            std::fs::write(f, b"rlib one").unwrap();
        }
        std::fs::write(&import, b"import library").unwrap();
        let built = [cdylib.clone(), import.clone(), rlib.clone()];
        assert_eq!(dep_infos_of(&built, "ambulo"), [one.join("ambulo.d")], "once, for both libraries");
        assert_eq!(dep_infos_of(&[import], "ambulo"), none, "no library among the files");
        std::fs::write(&cdylib, b"put there by hand").unwrap();
        assert_eq!(find_dep_infos(&rlib, "ambulo"), [one.join("ambulo.d")]);
        assert_eq!(dep_infos_of(&built, "ambulo"), none, "the rlib's answer is not the library's");
        assert_eq!(dep_infos_of(&[rlib.clone(), cdylib.clone()], "ambulo"), none, "in either order");
        // More folders than the search reads: no answer, never the ones it happened to reach.
        assert_eq!(rustc_folders(&profile, 4096).map(|f| f.len()), Some(4), "deps and three units' out folders");
        assert_eq!(rustc_folders(&profile, 3), None, "four folders, a bound of three");
        assert_eq!(rustc_folders(&profile, 1), None, "two packages, a bound of one");
        let _ = std::fs::remove_dir_all(&root);
    }

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

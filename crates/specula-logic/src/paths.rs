//! Paths a module's source names at compile time: `#[path = "..."]` on a `mod`, and the
//! `include_str!` / `include_bytes!` / `include!` macros. They must stay inside the module's own
//! folder (its `src/` and its top-level files, like ambulo's `actions.json`; never `target/`): a
//! module may not compile engine sources into itself or read files elsewhere
//! (`C:/Windows/win.ini`) at build time. Checked by [`crate::build::vet`] before every build and
//! by the engine at every commit of a mirrored `.rs` file.

use std::path::Path;

/// One refused path: 1-based line and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathProblem {
    pub line: usize,
    pub message: String,
}

const MACROS: [&str; 3] = ["include_str!", "include_bytes!", "include!"];

/// The string literal at the start of `s` (after whitespace): `"..."` or `r"..."` / `r#"..."#`.
/// `None` when something else is there (a `concat!`, an `env!`, a constant).
fn literal(s: &str) -> Option<String> {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(out),
                '\\' => {
                    let e = chars.next()?;
                    out.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                }
                c => out.push(c),
            }
        }
        return None;
    }
    let rest = s.strip_prefix('r')?;
    let hashes = rest.chars().take_while(|c| *c == '#').count();
    let body = rest[hashes..].strip_prefix('"')?;
    let end = format!("\"{}", "#".repeat(hashes));
    body.find(&end).map(|i| body[..i].to_owned())
}

/// Where `target` (as written in `file`, a path inside the module like `src/a/b.rs`) lands,
/// relative to the module; `None` when it is absolute or climbs out of the module.
fn resolve(file: &str, target: &str) -> Option<String> {
    let t = target.replace('\\', "/");
    if t.starts_with('/') || t.contains(':') {
        return None;
    }
    let mut parts: Vec<&str> = file.split('/').collect();
    parts.pop(); // the file's own name
    for seg in t.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// Problems with the compile-time paths in `text`, the module file at `rel_file` (relative to the
/// module, `/`-separated, e.g. `src/lib.rs`).
pub fn check_source(rel_file: &str, text: &str) -> Vec<PathProblem> {
    let mut out = Vec::new();
    let inside = |target: &str| resolve(rel_file, target).is_some_and(|r| !r.is_empty() && !r.starts_with("target/") && !r.starts_with(".cargo/") && r != "target");
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split("//").next().unwrap_or("");
        for m in MACROS {
            let mut from = 0;
            while let Some(at) = line[from..].find(m) {
                let pos = from + at;
                from = pos + m.len();
                // `my_include!` is another macro.
                if line[..pos].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let args = line[from..].trim_start();
                let Some(args) = args.strip_prefix(['(', '[', '{']) else { continue };
                match literal(args) {
                    Some(t) if inside(&t) => {}
                    Some(t) => out.push(PathProblem { line: i + 1, message: format!("{m}(\"{t}\") reaches outside the module's folder: modules may only include their own files") }),
                    None => out.push(PathProblem { line: i + 1, message: format!("{m} takes a plain string literal in a module (a path built with concat!/env! could reach anywhere)") }),
                }
            }
        }
        let mut from = 0;
        while let Some(at) = line[from..].find("path") {
            let pos = from + at;
            from = pos + 4;
            let before = line[..pos].trim_end();
            if !(before.ends_with("#[") || before.ends_with("#![")) {
                continue;
            }
            let rest = line[from..].trim_start();
            let Some(rest) = rest.strip_prefix('=') else { continue };
            match literal(rest) {
                Some(t) if inside(&t) => {}
                Some(t) => out.push(PathProblem { line: i + 1, message: format!("#[path = \"{t}\"] reaches outside the module's folder: a module may only compile its own files") }),
                None => out.push(PathProblem { line: i + 1, message: "#[path] takes a plain string literal".into() }),
            }
        }
    }
    out
}

/// [`check_source`] over every `.rs` file under the module's `src/`: the first problem as
/// `(code, message)`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_time_paths_stay_in_src() {
        let ok = "#[path = \"inner/x.rs\"]\nmod x;\nconst A: &str = include_str!(\"data.txt\");\nconst B: &[u8] = include_bytes!(r\"assets/b.bin\");\n// include_str!(\"../../x\") in a comment\nfn my_include() {}\n";
        assert!(check_source("src/lib.rs", ok).is_empty(), "{:?}", check_source("src/lib.rs", ok));
        assert!(check_source("src/a/b.rs", "include!(\"../c.rs\");").is_empty(), "src/c.rs is inside");
        assert!(check_source("src/lib.rs", "actions: include_str!(\"../actions.json\"),").is_empty(), "ambulo's own top-level file");
        assert_eq!(check_source("src/lib.rs", "include_bytes!(\"../target/debug/x.dll\")").len(), 1, "never the build output");
        for bad in [
            "#[path = \"../../../crates/specula/src/lib.rs\"]\nmod e;",
            "const W: &str = include_str!(\"C:/Windows/win.ini\");",
            "const W: &str = include_str!(\"/etc/passwd\");",
            "include!(\"../../build.rs\");",
            "const P: &[u8] = include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../x\"));",
            "#![path = \"..\\\\..\\\\x.rs\"]",
        ] {
            let p = check_source("src/lib.rs", bad);
            assert_eq!(p.len(), 1, "{bad}: {p:?}");
            assert_eq!(p[0].line, 1);
        }
    }
}

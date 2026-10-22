//! Files agents name in tool arguments (`lex_import {file}`, `desync_compare {a, b}`): only paths
//! relative to the engine's own folders (its captures, worlds and assets roots), never absolute,
//! UNC (`\\host\share`, which would hand out the user's NTLM credentials), verbatim (`\\?\`) or
//! drive-relative (`C:x`) paths, no `..`, no links or reparse points anywhere on the way, and
//! read with a bounded read. An answer never says whether a path outside the roots exists: such a
//! path is refused before anything touches the disk, with one `path_not_allowed`.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// Why a named file can't be read: a code agents branch on, and a message.
#[derive(Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
}

fn refuse(code: &'static str, message: impl Into<String>) -> Refusal {
    Refusal { code, message: message.into() }
}

fn not_allowed(what: &str) -> Refusal {
    refuse(
        "path_not_allowed",
        format!("{what} must be a path relative to this engine's captures, worlds or assets folder (no absolute, UNC or drive paths, no '..', no links), such as lex_export's import_file or a trace file's name"),
    )
}

/// The engine folders agents may name files in, in the order they are tried.
#[derive(Clone, Debug, Default)]
pub struct Roots(pub Vec<PathBuf>);

impl Roots {
    /// The folders `lex_import {file}` reads from, in order: this run's captures, the captures
    /// root, the worlds folder, and the assets root only when one was named (`--assets` or the
    /// hosted game's). The engine's fallback assets root is its working folder, which may hold
    /// anything, so it is left out.
    pub fn for_files(captures: &Path, captures_root: &Path, worlds: &Path, named_assets: Option<&Path>) -> Roots {
        let mut roots = vec![captures.to_path_buf(), captures_root.to_path_buf(), worlds.to_path_buf()];
        roots.extend(named_assets.map(Path::to_path_buf));
        Roots(roots)
    }

    /// Resolves `name` (relative, `ext` at the end) to an existing regular file under a root:
    /// the first root that has it. `what` names the argument in messages.
    pub fn resolve(&self, name: &str, ext: &str, what: &str) -> Result<PathBuf, Refusal> {
        let rel = Path::new(name);
        // Checked as text first: nothing outside the roots is ever looked up.
        let lexically_ok = !name.is_empty()
            && name.len() <= 512
            && !name.contains(':')
            && !name.starts_with('\\')
            && !name.starts_with('/')
            && !name.contains('\0')
            && !rel.is_absolute()
            && !rel.has_root()
            && rel.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
            && name.split(['/', '\\']).all(|part| part != "..");
        if !lexically_ok {
            return Err(not_allowed(what));
        }
        if !name.to_ascii_lowercase().ends_with(ext) {
            return Err(refuse("invalid_argument", format!("{what} must name a *{ext} file")));
        }
        for root in &self.0 {
            let Ok(root) = root.canonicalize() else { continue };
            if !no_links_below(&root, rel) {
                // A link on the way could lead anywhere: refused like any path outside the roots.
                return Err(not_allowed(what));
            }
            let path = root.join(rel);
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            if !meta.is_file() {
                continue;
            }
            match path.canonicalize() {
                Ok(c) if c.starts_with(&root) => return Ok(c),
                _ => return Err(not_allowed(what)),
            }
        }
        Err(refuse("not_found", format!("no file {name:?} under this engine's captures, worlds or assets folder")))
    }
}

/// Whether every existing step from `root` along `rel` is a plain file or folder (no symlink,
/// junction or other reparse point).
fn no_links_below(root: &Path, rel: &Path) -> bool {
    let mut at = root.to_path_buf();
    for c in rel.components() {
        if let Component::Normal(part) = c {
            at.push(part);
            match std::fs::symlink_metadata(&at) {
                Ok(m) if is_link(&m) => return false,
                Ok(_) => {}
                Err(_) => return true, // the rest doesn't exist: nothing to follow
            }
        }
    }
    true
}

fn is_link(m: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    m.file_type().is_symlink()
}

/// Reads at most `max` bytes of text from `path` (a bounded read: a bigger file is refused
/// without reading past `max`).
pub fn read_bounded(path: &Path, max: usize, what: &str) -> Result<String, Refusal> {
    let file = std::fs::File::open(path).map_err(|e| refuse("io", format!("could not read {what}: {}", e.kind())))?;
    let mut buf = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut buf).map_err(|e| refuse("io", format!("could not read {what}: {}", e.kind())))?;
    if buf.len() > max {
        return Err(refuse("too_large", format!("{what} is more than {max} bytes")));
    }
    String::from_utf8(buf).map_err(|_| refuse("invalid_document", format!("{what} is not UTF-8 text")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("specula-safe-path-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The review probe: absolute, UNC, verbatim, drive-relative and `..` paths are refused with
    /// one code whether or not they exist; files under a root resolve, the first root first.
    #[test]
    fn only_files_under_the_roots_resolve() {
        let base = tmp("roots");
        let (caps, worlds, outside) = (base.join("caps"), base.join("worlds"), base.join("data"));
        for d in [&caps, &worlds, &outside, &caps.join("sub")] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(outside.join("secret.json"), r#"{"password": "hunter2"}"#).unwrap();
        std::fs::write(caps.join("a.json"), "{}").unwrap();
        std::fs::write(worlds.join("a.json"), "{\"w\": 1}").unwrap();
        std::fs::write(caps.join("sub").join("b.json"), "{}").unwrap();
        let roots = Roots(vec![caps.clone(), worlds.clone()]);
        let secret = outside.join("secret.json").display().to_string();
        let missing = outside.join("nope.json").display().to_string();
        for bad in [
            secret.as_str(),
            missing.as_str(),
            "../data/secret.json",
            "sub/../../data/secret.json",
            r"\\host\share\x.json",
            r"\\?\C:\x.json",
            "C:x.json",
            "/etc/x.json",
            r"\x.json",
            "a.json:stream",
            "",
        ] {
            let e = roots.resolve(bad, ".json", "'file'").unwrap_err();
            assert_eq!(e.code, "path_not_allowed", "{bad}: {e:?}");
            assert!(!e.message.contains("hunter2"));
        }
        let got = roots.resolve("a.json", ".json", "'file'").unwrap();
        assert_eq!(got, caps.join("a.json").canonicalize().unwrap(), "captures first");
        assert!(roots.resolve("sub/b.json", ".json", "'file'").is_ok());
        assert_eq!(roots.resolve("zzz.json", ".json", "'file'").unwrap_err().code, "not_found");
        assert_eq!(roots.resolve("a.txt", ".json", "'file'").unwrap_err().code, "invalid_argument");
        // A bounded read.
        std::fs::write(caps.join("big.json"), "x".repeat(100)).unwrap();
        let p = roots.resolve("big.json", ".json", "'file'").unwrap();
        assert_eq!(read_bounded(&p, 99, "'file'").unwrap_err().code, "too_large");
        assert_eq!(read_bounded(&p, 100, "'file'").unwrap().len(), 100);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The assets root is a root only when it was named: the working-folder fallback is not.
    #[test]
    fn the_fallback_assets_root_is_not_a_root() {
        let base = tmp("assets");
        let (root, worlds, cwd) = (base.join("captures"), base.join("worlds"), base.join("cwd"));
        let caps = root.join("run-1");
        for d in [&caps, &worlds, &cwd] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(cwd.join("private.json"), "{}").unwrap();
        let fallback = Roots::for_files(&caps, &root, &worlds, None);
        assert!(!fallback.0.contains(&cwd));
        assert_eq!(fallback.resolve("private.json", ".json", "'file'").unwrap_err().code, "not_found");
        let named = Roots::for_files(&caps, &root, &worlds, Some(&cwd));
        assert_eq!(named.0.last(), Some(&cwd), "a named assets root comes last");
        assert!(named.resolve("private.json", ".json", "'file'").is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A link (or junction) under a root is refused, whatever it points at.
    #[test]
    fn links_under_a_root_are_refused() {
        let base = tmp("links");
        let (caps, outside) = (base.join("caps"), base.join("data"));
        std::fs::create_dir_all(&caps).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.json"), "{}").unwrap();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&outside, caps.join("ln")).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside, caps.join("ln")).is_ok();
        #[cfg(not(any(windows, unix)))]
        let made = false;
        if !made {
            eprintln!("links_under_a_root_are_refused: SKIPPED — this account can't make symlinks");
            let _ = std::fs::remove_dir_all(&base);
            return;
        }
        let roots = Roots(vec![caps.clone()]);
        assert_eq!(roots.resolve("ln/secret.json", ".json", "'file'").unwrap_err().code, "path_not_allowed");
        let _ = std::fs::remove_dir_all(&base);
    }
}

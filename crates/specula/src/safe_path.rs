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

fn capture_not_allowed(what: &str) -> Refusal {
    refuse(
        "path_not_allowed",
        format!("{what} must be a file in this engine's captures folder: the image.path (or packet path) a sight or studio tool returned, or a path relative to the captures folder (no UNC or drive-relative paths, no '..', no links)"),
    )
}

/// Whether `path` is `root` or below it, compared component by component (case-insensitively on
/// Windows), without touching the disk.
fn lexically_under(path: &Path, root: &Path) -> bool {
    let key = |c: Component| {
        let s = c.as_os_str().to_string_lossy().into_owned();
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s
        }
    };
    let (mut p, r) = (path.components().filter(|c| *c != Component::CurDir), root.components().filter(|c| *c != Component::CurDir));
    r.into_iter().all(|rc| p.next().is_some_and(|pc| key(pc) == key(rc)))
}

/// A capture or packet file an agent names (`review_submit`'s captures, which the window's review
/// screen later decodes): the absolute path a sight or studio tool returned, or a path relative to
/// this run's captures (`run`) or the captures `root`. It must be a regular file inside `root`
/// both as written and canonicalised, with no link or junction on the way. UNC, verbatim and
/// drive-relative paths, `..` and paths lexically outside `root` are refused before anything
/// touches the disk. Returns the absolute (not verbatim) path. (M4 audit: any absolute path was
/// accepted, and the review screen decoded it.)
pub fn capture_file(root: &Path, run: &Path, name: &str, what: &str) -> Result<PathBuf, Refusal> {
    let p = Path::new(name);
    if !name_ok(name) {
        return Err(capture_not_allowed(what));
    }
    let root_abs = std::path::absolute(root).map_err(|_| capture_not_allowed(what))?;
    // A relative name: in this run's captures, the captures root, or relative to the engine's
    // working folder (a relative `--captures` makes sight tools return such paths), the last
    // only when that is inside the root.
    let candidates: Vec<PathBuf> = if p.is_absolute() { vec![p.to_path_buf()] } else { vec![run.join(p), root.join(p), p.to_path_buf()] };
    for c in candidates {
        let abs = std::path::absolute(&c).map_err(|_| capture_not_allowed(what))?;
        if !lexically_under(&abs, &root_abs) {
            if p.is_absolute() {
                return Err(capture_not_allowed(what));
            }
            continue;
        }
        // What is below the root (its components matched above, whatever their case).
        let rel: PathBuf = abs.components().filter(|c| *c != Component::CurDir).skip(root_abs.components().filter(|c| *c != Component::CurDir).count()).collect();
        if !no_links_below(&root_abs, &rel) {
            return Err(capture_not_allowed(what));
        }
        let Ok(meta) = std::fs::symlink_metadata(&abs) else { continue };
        if !meta.is_file() {
            continue;
        }
        return match (abs.canonicalize(), root_abs.canonicalize()) {
            (Ok(c), Ok(r)) if c.starts_with(&r) => Ok(abs),
            _ => Err(capture_not_allowed(what)),
        };
    }
    Err(refuse("capture_missing", format!("{what} {name:?} is not a file in this engine's captures folder (pass the image.path a sight tool returned)")))
}

/// The folders a review's files may be in (see [`review_file`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReviewRoots {
    /// The captures folder and this run's captures in it.
    pub captures: PathBuf,
    pub run: PathBuf,
    /// The worlds folder, and the current queue's folder in it (the world's, or this run's
    /// unsaved one): `evidence/<file>` names a file in its `evidence` folder.
    pub worlds: PathBuf,
    pub queue: PathBuf,
}

/// A file a review submission or critique names: a capture ([`capture_file`]), or review
/// evidence (`<worlds>/<world>/evidence/<file>`, or `evidence/<file>` in the current queue's
/// folder), and nothing else: never a world's own files, never a path outside these. The same
/// lexical checks as [`capture_file`] come first, so a UNC, verbatim or `..` name is refused
/// before anything touches the disk (no credentials sent, no existence answered); then no link
/// on the way, a regular file, inside both as written and canonicalised. Returns the absolute
/// (not verbatim) path. Used by review_submit, critique_add, the review screen and every
/// resolution of a review's paths.
pub fn review_file(r: &ReviewRoots, name: &str, what: &str) -> Result<PathBuf, Refusal> {
    if !name_ok(name) {
        return Err(capture_not_allowed(what));
    }
    let worlds = std::path::absolute(&r.worlds).map_err(|_| capture_not_allowed(what))?;
    // `evidence/<file>`: the current queue's evidence.
    let rel = name.strip_prefix("evidence/").or_else(|| name.strip_prefix("evidence\\"));
    let target = match rel {
        Some(file) => {
            let queue = std::path::absolute(&r.queue).map_err(|_| capture_not_allowed(what))?;
            Some(queue.join("evidence").join(file))
        }
        None => {
            let abs = std::path::absolute(name).map_err(|_| capture_not_allowed(what))?;
            (Path::new(name).is_absolute() && lexically_under(&abs, &worlds)).then_some(abs)
        }
    };
    let Some(abs) = target else { return capture_file(&r.captures, &r.run, name, what) };
    // Exactly <worlds>/<a world or unsaved folder>/evidence/<file>: never a world's own files.
    if !lexically_under(&abs, &worlds) {
        return Err(capture_not_allowed(what));
    }
    let rel: PathBuf = abs.components().filter(|c| *c != Component::CurDir).skip(worlds.components().filter(|c| *c != Component::CurDir).count()).collect();
    let parts: Vec<Component> = rel.components().collect();
    let evidence_file = parts.len() == 3 && parts.iter().all(|c| matches!(c, Component::Normal(_))) && parts[1].as_os_str() == "evidence";
    if !evidence_file || !no_links_below(&worlds, &rel) {
        return Err(capture_not_allowed(what));
    }
    let missing = || refuse("capture_missing", format!("{what} {name:?} is not a file in the review's evidence (it may have been removed)"));
    let meta = std::fs::symlink_metadata(&abs).map_err(|_| missing())?;
    if !meta.is_file() {
        return Err(missing());
    }
    let folder = abs.parent().map(Path::to_path_buf).unwrap_or_default();
    match (abs.canonicalize(), folder.canonicalize()) {
        (Ok(c), Ok(f)) if c.starts_with(&f) => Ok(abs),
        _ => Err(capture_not_allowed(what)),
    }
}

/// The lexical part of [`capture_file`]'s checks: nothing here touches the disk.
fn name_ok(name: &str) -> bool {
    let p = Path::new(name);
    !name.is_empty()
        && name.len() <= 512
        && !name.contains('\0')
        && !name.starts_with("\\\\")
        && !name.starts_with("//")
        // A rooted name without a drive (`\x.png`) on every platform, not only Windows, where it
        // means the current drive's root (on Linux it was a file name: the same rule everywhere).
        && !name.starts_with('\\')
        && (!p.has_root() || p.is_absolute())
        && p.components().all(|c| match c {
            Component::Prefix(px) => matches!(px.kind(), std::path::Prefix::Disk(_)),
            Component::ParentDir => false,
            _ => true,
        })
        && name.split(['/', '\\']).all(|part| part != "..")
        // A ':' only as a drive letter's (no alternate data streams, no `C:x`).
        && name.char_indices().filter(|(_, c)| *c == ':').all(|(i, _)| i == 1 && p.is_absolute())
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

    /// M4 audit (probe_capture_path.py): review_submit took captures and packets at any absolute
    /// path, and the review screen decoded them. Only files in the captures folder pass.
    #[test]
    fn captures_must_be_in_the_captures_folder() {
        let base = tmp("captures");
        let (root, outside) = (base.join("captures"), base.join("data"));
        let run = root.join("run-1");
        for d in [&run, &outside, &root.join("run-0")] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(run.join("a.png"), b"png").unwrap();
        std::fs::write(root.join("run-0").join("old.png"), b"png").unwrap();
        std::fs::write(outside.join("secret.png"), b"png").unwrap();
        let ok = |name: &str| capture_file(&root, &run, name, "captures[0]");
        // The absolute path a sight tool returned, relative to this run, or to the root.
        let abs = run.join("a.png").display().to_string();
        assert_eq!(ok(&abs).unwrap(), std::path::absolute(run.join("a.png")).unwrap());
        assert!(ok("a.png").is_ok() && ok("run-0/old.png").is_ok() && ok("./a.png").is_ok());
        let secret = outside.join("secret.png").display().to_string();
        let sneaky = format!("{}/../data/secret.png", root.display());
        for bad in [secret.as_str(), sneaky.as_str(), "../data/secret.png", "run-1/../../data/secret.png", r"\\host\share\x.png", r"\\?\C:\x.png", "//host/share/x.png", "C:x.png", "a.png:stream", r"\x.png", ""] {
            assert_eq!(ok(bad).unwrap_err().code, "path_not_allowed", "{bad}");
        }
        assert_eq!(ok("nope.png").unwrap_err().code, "capture_missing");
        assert_eq!(ok("run-1").unwrap_err().code, "capture_missing", "a folder is not a capture");
        // Review files (round-3 review): captures, and evidence files in a world's evidence folder,
        // never a world's own files; UNC and verbatim names refused before any lookup.
        let worlds = base.join("worlds");
        for d in [worlds.join("mine").join("evidence"), worlds.join("other").join("evidence")] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(worlds.join("mine").join("evidence").join("e.png"), b"png").unwrap();
        std::fs::write(worlds.join("other").join("evidence").join("o.png"), b"png").unwrap();
        std::fs::write(worlds.join("other").join("world.json"), b"{}").unwrap();
        std::fs::write(worlds.join("other").join("notes.txt"), b"private").unwrap();
        let roots = ReviewRoots { captures: root.clone(), run: run.clone(), worlds: worlds.clone(), queue: worlds.join("mine") };
        let rv = |name: &str| review_file(&roots, name, "captures[0]");
        assert!(rv("a.png").is_ok() && rv(&abs).is_ok(), "captures as before");
        assert_eq!(rv("evidence/e.png").unwrap(), std::path::absolute(worlds.join("mine").join("evidence").join("e.png")).unwrap(), "relative to the queue");
        assert!(rv(&worlds.join("other").join("evidence").join("o.png").display().to_string()).is_ok(), "another world's evidence (adopting a copied world)");
        for bad in [
            worlds.join("other").join("world.json").display().to_string(),
            worlds.join("other").join("notes.txt").display().to_string(),
            worlds.join("other").join("evidence").display().to_string(),
            "evidence/../world.json".to_string(),
            r"\\host\share\evidence\x.png".to_string(),
            r"\\?\C:\x.png".to_string(),
        ] {
            assert_eq!(rv(&bad).unwrap_err().code, "path_not_allowed", "{bad}");
        }
        assert_eq!(rv("evidence/gone.png").unwrap_err().code, "capture_missing");
        // A link (or junction) inside the captures folder that leads out is refused.
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&outside, run.join("ln")).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside, run.join("ln")).is_ok();
        #[cfg(not(any(windows, unix)))]
        let made = false;
        if made {
            assert_eq!(ok("ln/secret.png").unwrap_err().code, "path_not_allowed");
            assert_eq!(ok(&run.join("ln").join("secret.png").display().to_string()).unwrap_err().code, "path_not_allowed");
        } else {
            eprintln!("captures_must_be_in_the_captures_folder: link part SKIPPED — this account can't make symlinks");
        }
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

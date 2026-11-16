//! The digest of a module's folder: what a proposal names and an approval pins
//! (docs/design/MODULES.md, "Who may load a module").
//!
//! A builder's `logic_build` becomes a proposal carrying this digest; the owner or a granted
//! lead reviews the files and builds with `approve`, which is refused (`sources_changed`) when
//! the folder no longer hashes the same, before the build and again after it, so nobody can swap
//! the sources between the review and the load.
//!
//! Every file under the folder counts (any extension: `#[path]` and `include!` can compile a
//! `.txt`), except `target/`, folders whose names start with a dot, and the top-level `Cargo.lock`
//! (Cargo writes it during a build; with path crates only it is derived from the manifests). Bounded: at most
//! [`MAX_FILES`] files, [`MAX_DIRS`] folders and [`MAX_BYTES`] in all; past any cap the digest is
//! refused (`too_big`), never taken over part of the folder. Links are refused too (`link`): what
//! they point at could change without the folder changing.

use std::path::{Path, PathBuf};

pub const MAX_FILES: usize = 1024;
pub const MAX_DIRS: usize = 512;
pub const MAX_BYTES: u64 = 16 << 20;

/// A folder's digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Digest {
    /// SHA-256 (the operating system's) over every file's relative path, length and bytes, in
    /// path order: lowercase hex.
    pub hex: String,
    pub files: usize,
    pub bytes: u64,
}

/// The files the digest covers, sorted by their relative paths (`/`-separated), or why not.
pub fn files(dir: &Path) -> Result<Vec<(String, PathBuf)>, (&'static str, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    let mut walked = 0usize;
    let mut bytes = 0u64;
    while let Some(d) = stack.pop() {
        walked += 1;
        if walked > MAX_DIRS {
            return Err(("too_big", format!("the module folder has more than {MAX_DIRS} folders: too many to digest (a proposal must cover every file)")));
        }
        let entries = std::fs::read_dir(&d).map_err(|e| ("unreadable", format!("can't list {}: {e}", d.display())))?;
        for e in entries {
            let e = e.map_err(|e| ("unreadable", format!("can't list {}: {e}", d.display())))?;
            let name = e.file_name().to_string_lossy().into_owned();
            let p = e.path();
            let meta = std::fs::symlink_metadata(&p).map_err(|e| ("unreadable", format!("can't read {}: {e}", p.display())))?;
            if meta.file_type().is_symlink() || is_reparse(&meta) {
                return Err(("link", format!("{} is a link: a module folder may not hold links (what they point at could change unseen)", p.display())));
            }
            if meta.is_dir() {
                if (d == dir && name == "target") || name.starts_with('.') {
                    continue;
                }
                stack.push(p);
            } else if meta.is_file() && !(d == dir && name == "Cargo.lock") {
                bytes += meta.len();
                if out.len() >= MAX_FILES || bytes > MAX_BYTES {
                    return Err(("too_big", format!("the module folder holds more than {MAX_FILES} files or {} MiB: too much to digest", MAX_BYTES >> 20)));
                }
                let rel = p.strip_prefix(dir).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                out.push((rel, p));
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(windows)]
fn is_reparse(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    // FILE_ATTRIBUTE_REPARSE_POINT: junctions and other links that aren't symlinks.
    meta.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse(_: &std::fs::Metadata) -> bool {
    false
}

/// When each digested file was last written: its relative path, length and modification time, in
/// path order. Two snapshots with equal stamps saw no write in between, which a digest alone
/// can't say (sources swapped for a build and put back hash the same).
pub type Stamps = Vec<(String, u64, Option<std::time::SystemTime>)>;

/// The digest of the module folder `dir` (see the module docs).
pub fn module_digest(dir: &Path) -> Result<Digest, (&'static str, String)> {
    module_snapshot(dir).map(|(d, _)| d)
}

/// [`module_digest`] with each file's [`Stamps`], read before its bytes (a write between the two
/// reads shows as a changed stamp the next time).
pub fn module_snapshot(dir: &Path) -> Result<(Digest, Stamps), (&'static str, String)> {
    let list = files(dir)?;
    let mut buf = Vec::new();
    let mut stamps = Vec::with_capacity(list.len());
    for (rel, path) in &list {
        let meta = std::fs::metadata(path).map_err(|e| ("unreadable", format!("can't read {}: {e}", path.display())))?;
        stamps.push((rel.clone(), meta.len(), meta.modified().ok()));
        let bytes = std::fs::read(path).map_err(|e| ("unreadable", format!("can't read {}: {e}", path.display())))?;
        if buf.len() as u64 + bytes.len() as u64 > MAX_BYTES + (MAX_FILES as u64) * 512 {
            return Err(("too_big", format!("the module folder grew past {} MiB while it was read", MAX_BYTES >> 20)));
        }
        buf.extend_from_slice(rel.as_bytes());
        buf.push(0);
        buf.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        buf.extend_from_slice(&bytes);
    }
    let d = specula_sys::digest::sha256(&buf).map_err(|e| ("digest_unavailable", format!("no SHA-256 here ({e}): proposals can't be pinned on this platform")))?;
    Ok((Digest { hex: specula_sys::digest::hex(&d), files: list.len(), bytes: buf.len() as u64 }, stamps))
}

/// What configures every build in the project `root`, besides a module's own folder: the root's
/// `Cargo.toml`, a toolchain file there, and everything under its `.cargo/` (a wrapper, a
/// linker or a runner named there is a program the build runs).
const ROOT_FILES: [&str; 3] = ["Cargo.toml", "rust-toolchain", "rust-toolchain.toml"];

/// The digest of the build configuration at the project's root ([`ROOT_FILES`] and `.cargo/`):
/// what the start-up build holds a project to beside each module's own digest
/// (`specula::approved`). A module's folder digest does not cover it, and `vet` leaves the
/// root's own configuration to the project. A file that is not there counts as not there (the
/// digest of a root with none of them is a digest too); a link, or more than the bounds of a
/// module folder, is refused.
pub fn root_config_digest(root: &Path) -> Result<Digest, (&'static str, String)> {
    let mut list: Vec<(String, PathBuf)> = Vec::new();
    for name in ROOT_FILES {
        let path = root.join(name);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() || is_reparse(&meta) => return Err(("link", format!("{} is a link: what it points at could change unseen", path.display()))),
            Ok(meta) if meta.is_file() => list.push((name.to_owned(), path)),
            Ok(_) => return Err(("unreadable", format!("{} is not a file", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(("unreadable", format!("can't read {}: {e}", path.display()))),
        }
    }
    let config = root.join(".cargo");
    match std::fs::symlink_metadata(&config) {
        Ok(meta) if meta.file_type().is_symlink() || is_reparse(&meta) => return Err(("link", format!("{} is a link: what it points at could change unseen", config.display()))),
        Ok(meta) if meta.is_dir() => list.extend(files(&config)?.into_iter().map(|(rel, path)| (format!(".cargo/{rel}"), path))),
        // (A file named `.cargo` configures nothing; it is told apart from no `.cargo` at all.)
        Ok(_) => list.push((".cargo".to_owned(), config)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(("unreadable", format!("can't read {}: {e}", config.display()))),
    }
    list.sort();
    let mut buf = Vec::new();
    for (rel, path) in &list {
        let bytes = std::fs::read(path).map_err(|e| ("unreadable", format!("can't read {}: {e}", path.display())))?;
        if buf.len() as u64 + bytes.len() as u64 > MAX_BYTES {
            return Err(("too_big", format!("the project's build configuration is more than {} MiB: too much to digest", MAX_BYTES >> 20)));
        }
        buf.extend_from_slice(rel.as_bytes());
        buf.push(0);
        buf.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        buf.extend_from_slice(&bytes);
    }
    let d = specula_sys::digest::sha256(&buf).map_err(|e| ("digest_unavailable", format!("no SHA-256 here ({e}): a build's configuration can't be pinned on this platform")))?;
    Ok(Digest { hex: specula_sys::digest::hex(&d), files: list.len(), bytes: buf.len() as u64 })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The root's build configuration has a digest of its own: a wrapper named in `.cargo/`,
    /// a toolchain file or a changed workspace manifest changes it, a module's sources do not.
    #[test]
    fn the_roots_build_configuration_has_its_own_digest() {
        if !cfg!(windows) {
            // Skipped: the OS SHA-256 is Windows-only so far.
            return;
        }
        let root = std::env::temp_dir().join(format!("specula-digest-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("modules").join("m").join("src")).unwrap();
        let bare = root_config_digest(&root).unwrap();
        assert_eq!(bare.files, 0);
        std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = [\"modules/m\"]\n").unwrap();
        let with_manifest = root_config_digest(&root).unwrap();
        assert_ne!(with_manifest.hex, bare.hex);
        // A module's own sources are not the root's configuration.
        std::fs::write(root.join("modules").join("m").join("src").join("lib.rs"), "pub fn f() {}\n").unwrap();
        assert_eq!(root_config_digest(&root).unwrap().hex, with_manifest.hex);
        // A wrapper in `.cargo/` (any file there), and a toolchain file, each change it.
        std::fs::create_dir_all(root.join(".cargo")).unwrap();
        std::fs::write(root.join(".cargo").join("config.toml"), "[build]\nrustc-wrapper = \"wrap\"\n").unwrap();
        let with_config = root_config_digest(&root).unwrap();
        assert_ne!(with_config.hex, with_manifest.hex);
        assert_eq!(with_config.files, 2);
        std::fs::write(root.join("rust-toolchain.toml"), "[toolchain]\npath = \"evil\"\n").unwrap();
        let with_toolchain = root_config_digest(&root).unwrap();
        assert_ne!(with_toolchain.hex, with_config.hex);
        // Taken away again, the digest is what it was.
        std::fs::remove_file(root.join("rust-toolchain.toml")).unwrap();
        std::fs::remove_dir_all(root.join(".cargo")).unwrap();
        assert_eq!(root_config_digest(&root).unwrap().hex, with_manifest.hex);
        let _ = std::fs::remove_dir_all(&root);
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("specula-digest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        d
    }

    #[test]
    fn a_change_anywhere_in_the_folder_changes_the_digest() {
        if !cfg!(windows) {
            // Skipped: the OS SHA-256 is Windows-only so far.
            return;
        }
        let d = tmp("change");
        std::fs::write(d.join("Cargo.toml"), "[package]\n").unwrap();
        std::fs::write(d.join("src/lib.rs"), "fn a() {}\n").unwrap();
        std::fs::create_dir_all(d.join("target/debug")).unwrap();
        std::fs::write(d.join("target/debug/x.dll"), "build output is not source").unwrap();
        let a = module_digest(&d).unwrap();
        assert_eq!(a.files, 2, "target/ is left out");
        std::fs::write(d.join("Cargo.lock"), "# written by cargo during a build").unwrap();
        assert_eq!(module_digest(&d).unwrap(), a, "and the Cargo.lock a build writes");
        std::fs::write(d.join("target/debug/x.dll"), "rebuilt").unwrap();
        assert_eq!(module_digest(&d).unwrap(), a, "build output doesn't change it");
        // A file any extension can be compiled from (#[path = "imp.txt"]).
        std::fs::write(d.join("src/imp.txt"), "extern {}").unwrap();
        let b = module_digest(&d).unwrap();
        assert_ne!(a.hex, b.hex);
        // A rename with the same bytes changes it too.
        std::fs::rename(d.join("src/imp.txt"), d.join("src/imp2.txt")).unwrap();
        assert_ne!(module_digest(&d).unwrap().hex, b.hex);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Review R2 of m5-gmods: sources swapped while a build runs and put back before the second
    /// digest hash the same; the stamps show the writes.
    #[test]
    fn a_file_swapped_and_put_back_shows_in_the_stamps() {
        if !cfg!(windows) {
            // Skipped: the OS SHA-256 is Windows-only so far.
            return;
        }
        let d = tmp("swap");
        std::fs::write(d.join("Cargo.toml"), "[package]
").unwrap();
        std::fs::write(d.join("src/lib.rs"), "fn good() {}
").unwrap();
        let before = module_snapshot(&d).unwrap();
        assert_eq!(module_snapshot(&d).unwrap(), before, "nothing written: the same snapshot");
        // Cargo's own outputs don't count.
        std::fs::write(d.join("Cargo.lock"), "# cargo").unwrap();
        std::fs::create_dir_all(d.join("target")).unwrap();
        std::fs::write(d.join("target/out.d"), "x").unwrap();
        assert_eq!(module_snapshot(&d).unwrap(), before);
        // The swap and the restore, as a project write makes them (a new file renamed in).
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(d.join("src/lib.rs"), "fn evil() {}
").unwrap();
        std::fs::write(d.join("src/.lib.rs.tmp"), "fn good() {}
").unwrap();
        std::fs::rename(d.join("src/.lib.rs.tmp"), d.join("src/lib.rs")).unwrap();
        let after = module_snapshot(&d).unwrap();
        assert_eq!(after.0, before.0, "the bytes are back: the digest alone can't tell");
        assert_ne!(after.1, before.1, "the stamps can");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn caps_refuse_instead_of_digesting_part() {
        let d = tmp("caps");
        for i in 0..=MAX_DIRS {
            std::fs::create_dir_all(d.join(format!("src/d{i:04}"))).unwrap();
        }
        assert_eq!(files(&d).unwrap_err().0, "too_big");
        let _ = std::fs::remove_dir_all(&d);
    }
}

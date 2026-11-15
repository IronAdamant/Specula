//! Keeping a build folder between two runs of CI (owner, 2026-11-16: "CI keeps its build between
//! runs").
//!
//! Cargo tells a changed source of this workspace by its time: a source newer than what was
//! built from it is built again. A fresh checkout gives every file the time of the checkout, so
//! a build folder kept from the last run is of no use as it stands: everything looks changed.
//!
//! `examen kept --mark FILE` writes, after a build that passed, the commit the folder was built
//! at. `examen kept --built-at FILE`, before the next build, gives every tracked file that does
//! **not** differ from that commit an old time; a file that differs keeps the checkout's time
//! and is built again, with what depends on it.
//!
//! **Nothing is kept, and everything is built, whenever the times cannot be trusted to say
//! enough:** no file, not a commit git has, or a difference in something cargo does not tell by
//! a source's time (a manifest, the lock file, a `clippy.toml`, the toolchain file, cargo's
//! configuration, a workflow: `resets_the_build`). With `--clear FOLDER` the folder is then
//! removed too, so that no build of an older run is left in it and it does not grow.
//!
//! For CI's machines, whose checkout is thrown away. On a developer's checkout it would give
//! their files old times for no gain: cargo's own state there is already right. `--clear` does
//! nothing where `CI` is not set.
//!
//! **What this cannot show** is said in ROADMAP M6 ("Verifying takes less time"): it has not
//! run on GitHub, and it rests on the cache restoring files with the times they were saved
//! with. The weekly run of `newest.yml` builds from nothing and would show a kept folder hiding
//! something.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::cli::Args;
use crate::run::{root, said, said_text};

/// The time an unchanged file is given: 2020-01-01, before any build folder that can be kept.
const OLD: Duration = Duration::from_secs(1_577_836_800);

fn names(bytes: &[u8]) -> BTreeSet<String> {
    String::from_utf8_lossy(bytes).split('\0').filter(|n| !n.is_empty()).map(str::to_owned).collect()
}

/// The tracked files whose build is still right: every one that does not differ from the
/// commit the folder was built at.
fn unchanged(tracked: &BTreeSet<String>, differ: &BTreeSet<String>) -> Vec<String> {
    tracked.difference(differ).cloned().collect()
}

/// A commit's full name, as `--mark` wrote it.
fn commit_in(text: &str) -> Option<&str> {
    let sha = text.trim();
    (sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then_some(sha)
}

/// A differing path after which nothing of an earlier build is trusted. Cargo sees a changed
/// source by its time, and that is all the times can say: a manifest or a lock file can bring
/// back a build of an older run that is still in the folder (a feature switched on and off
/// again), clippy does not read its `clippy.toml` again for a crate whose sources did not
/// change, and a workflow can change the flags a build is made with (review R1 of this lane,
/// findings 4 and 5).
fn resets_the_build(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(name, "Cargo.toml" | "Cargo.lock" | "clippy.toml" | "rust-toolchain.toml" | "rust-toolchain") || path.starts_with(".cargo/") || path.starts_with(".github/")
}

/// Whether `dir` is a folder strictly inside the checkout: the root joined with a relative path
/// of plain names. `.`, `..`, an absolute path and a drive are not (review R2 of this lane: with
/// `CI` set, `--clear .` would have removed the checkout).
fn inside(dir: &Path) -> bool {
    use std::path::Component;
    match dir.strip_prefix(root()) {
        Ok(rest) => rest.components().count() > 0 && rest.components().all(|c| matches!(c, Component::Normal(_))),
        Err(_) => false,
    }
}

/// Removes the kept folder when nothing of it is used, so that no build of an older run is left
/// in it and it does not grow from run to run. Only on CI's machines (`CI` is set there): on a
/// developer's machine a build folder holds more than builds.
fn clear(dir: Option<&Path>) {
    let Some(dir) = dir else { return };
    if std::env::var_os("CI").is_none() {
        println!("{} is left as it is: a folder is cleared on CI's machines only (CI is not set)", dir.display());
    } else if !inside(dir) {
        println!("{} is left as it is: only a folder inside the checkout, named by a plain relative path, is cleared", dir.display());
    } else if dir.exists() {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => println!("{} was cleared", dir.display()),
            Err(e) => println!("{} could not be cleared: {e}", dir.display()),
        }
    }
}

fn built_at(file: &Path, folder: Option<&Path>, root: &Path) -> i32 {
    let limit = Duration::from_secs(300);
    let nothing = |why: String| {
        println!("no build is kept: {why}. Everything is built.");
        clear(folder);
        0
    };
    let Ok(text) = std::fs::read_to_string(file) else {
        return nothing(format!("{} is not there", file.display()));
    };
    let Some(sha) = commit_in(&text) else {
        return nothing(format!("{} names no commit", file.display()));
    };
    if !said(&["git", "cat-file", "-e", &format!("{sha}^{{commit}}")], root, limit).is_some_and(|(code, _)| code == 0) {
        return nothing(format!("git has no commit {sha}"));
    }
    // What differs between that commit and the files as they stand (committed or not). A moved
    // file is both its paths.
    let Some((0, differ)) = said(&["git", "diff", "--no-renames", "--name-only", "-z", sha], root, limit) else {
        return nothing("git did not say what differs".to_owned());
    };
    let Some((0, tracked)) = said(&["git", "ls-files", "-z"], root, limit) else {
        return nothing("git did not list its files".to_owned());
    };
    let (tracked, differ) = (names(&tracked), names(&differ));
    if let Some(path) = differ.iter().find(|p| resets_the_build(p)) {
        return nothing(format!("{path} differs from {sha}, and a build made before it changed is not trusted"));
    }
    let old = SystemTime::UNIX_EPOCH + OLD;
    let mut kept = 0;
    for name in unchanged(&tracked, &differ) {
        // (A file git lists and the checkout lacks is not there to be built from.)
        if let Ok(file) = std::fs::File::options().write(true).open(root.join(&name)) {
            kept += usize::from(file.set_modified(old).is_ok());
        }
    }
    println!("the build folder is of {sha}: {kept} file(s) have not changed since and keep their build; {} differ and are built again", differ.len());
    0
}

fn mark(file: &Path, root: &Path) -> i32 {
    let limit = Duration::from_secs(300);
    let head = said_text(&["git", "rev-parse", "HEAD"], root, limit);
    // A build of files that differ from the commit is not that commit's build.
    let clean = said_text(&["git", "status", "--porcelain", "--untracked-files=no"], root, limit).is_some_and(|s| s.trim().is_empty());
    match head.as_deref().and_then(commit_in) {
        Some(sha) if clean => {
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match std::fs::write(file, format!("{sha}\n")) {
                Ok(()) => {
                    println!("this build folder is of {sha}");
                    0
                }
                Err(e) => {
                    println!("{}: {e}", file.display());
                    1
                }
            }
        }
        _ => {
            let _ = std::fs::remove_file(file);
            println!("this build is of no commit (git gave none, or tracked files differ from it): the folder is not marked, and the next run builds everything");
            0
        }
    }
}

/// `examen kept --built-at FILE [--clear FOLDER] | --mark FILE`.
pub fn main(mut args: Args) -> i32 {
    let parsed = (|| -> Result<_, String> {
        let (at, mark, clear) = (args.value("--built-at")?, args.value("--mark")?, args.value("--clear")?);
        args.done()?;
        Ok((at, mark, clear))
    })();
    let root = root();
    match parsed {
        Ok((Some(file), None, clear)) => built_at(&root.join(file), clear.map(|c| root.join(c)).as_deref(), &root),
        Ok((None, Some(file), None)) => mark(&root.join(file), &root),
        Ok(_) => {
            println!("examen kept --built-at FILE [--clear FOLDER] (before a build) or --mark FILE (after one that passed)");
            2
        }
        Err(e) => {
            println!("{e}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn only_files_that_do_not_differ_keep_their_build() {
        let tracked = set(&["a.rs", "b.rs", "c.rs", "new.rs"]);
        // (A deleted file differs and is not tracked any more; a new one differs and is.)
        let differ = set(&["b.rs", "deleted.rs", "new.rs"]);
        assert_eq!(unchanged(&tracked, &differ), ["a.rs", "c.rs"]);
        assert!(unchanged(&tracked, &tracked).is_empty());
        assert_eq!(names(b"a.rs\0dir/b c.rs\0"), set(&["a.rs", "dir/b c.rs"]));
    }

    /// Review R1, findings 4 and 5: what cargo does not tell by a source's time.
    #[test]
    fn a_manifest_a_lint_file_or_a_workflow_resets_the_build() {
        for path in ["Cargo.toml", "Cargo.lock", "games/exemplum/Cargo.toml", "crates/specula-physics/clippy.toml", "rust-toolchain.toml", ".cargo/config.toml", ".github/workflows/ci.yml"] {
            assert!(resets_the_build(path), "{path}");
        }
        for path in ["crates/specula/src/lib.rs", "HANDOFF.md", "games/sepulcra/assets/style.toml", "crates/specula-gpu/shaders/mesh.frag.spv", "docs/Cargo.toml.md"] {
            assert!(!resets_the_build(path), "{path}");
        }
    }

    #[test]
    fn only_a_folder_inside_the_checkout_can_be_cleared() {
        let root = root();
        assert!(inside(&root.join("target")) && inside(&root.join("target").join("debug")));
        for outside in [root.clone(), root.join("."), root.join(".."), root.join("target").join("..").join(".."), std::path::PathBuf::from("/"), std::env::temp_dir()] {
            assert!(!inside(&outside), "{}", outside.display());
        }
    }

    /// A folder is cleared on CI's machines only: a developer's build folder holds more than
    /// builds (HANDOFF, "On the development laptop: what never to do").
    #[test]
    fn a_folder_is_not_cleared_where_ci_is_not_set() {
        if std::env::var_os("CI").is_some() {
            println!("skipped: CI is set here, where a folder is cleared");
            return;
        }
        let dir = std::env::temp_dir().join(format!("examen-kept-clear-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        clear(Some(&dir));
        assert!(dir.join("inner").is_dir(), "the folder is still there");
        assert_eq!(built_at(&dir.join("absent"), Some(&dir), &root()), 0);
        assert!(dir.join("inner").is_dir(), "and after a run that kept nothing");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_mark_is_a_commits_full_name_or_nothing() {
        assert_eq!(commit_in("0123456789abcdef0123456789abcdef01234567\n"), Some("0123456789abcdef0123456789abcdef01234567"));
        assert_eq!(commit_in("0123456"), None, "a short name could be another commit's");
        assert_eq!(commit_in("main"), None);
        assert_eq!(commit_in(""), None);
        assert_eq!(commit_in("0123456789abcdef0123456789abcdef0123456g"), None);
    }

    /// Whenever the commit cannot be used, no file's time is touched.
    #[test]
    fn a_mark_that_cannot_be_used_touches_nothing() {
        let root = root();
        let dir = std::env::temp_dir().join(format!("examen-kept-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = root.join("Cargo.toml");
        let before = std::fs::metadata(&manifest).unwrap().modified().unwrap();
        assert_eq!(built_at(&dir.join("absent"), None, &root), 0);
        std::fs::write(dir.join("words"), "not a commit\n").unwrap();
        assert_eq!(built_at(&dir.join("words"), None, &root), 0);
        // A well-formed name of a commit git does not have.
        std::fs::write(dir.join("unknown"), "0123456789abcdef0123456789abcdef01234567\n").unwrap();
        assert_eq!(built_at(&dir.join("unknown"), None, &root), 0);
        assert_eq!(std::fs::metadata(&manifest).unwrap().modified().unwrap(), before, "no file's time moved");
        let _ = std::fs::remove_dir_all(dir);
    }
}

//! The last commit the gate passed at, in this checkout: what the next gate here is measured
//! from.
//!
//! Without it the gate measures from `origin/main`, and while work is not pushed (it is pushed
//! only when the owner asks) that is everything since the last push: a note added to a document
//! ran the whole gate again, because the commits under it changed the engine. CI has the same
//! problem and the same answer (`ci.yml`, the `green` job).
//!
//! **Why it is sound.** A commit is recorded only when the gate passed with nothing uncommitted,
//! and only for a run that chose its own steps (by what changed, or everything). Each such run
//! verified what its change needed, measured from the commit recorded before it; so everything
//! between the last full run and the recorded commit has been run at the size it needed. A
//! recorded commit that is not an ancestor of what is checked out (history was rewritten, another
//! branch) is not used, and the gate measures from `origin/main` again.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::run::{said, said_text, target_dir};

const LIMIT: Duration = Duration::from_secs(120);

/// Where this checkout's record is kept: in the build folder, by the checkout's own path (two
/// checkouts may share a build folder).
fn file(root: &Path) -> PathBuf {
    let key = specula_core::hash::hash_str(&root.to_string_lossy().replace('\\', "/").to_lowercase());
    target_dir().join("gate").join(format!("passed-{key:016x}.txt"))
}

fn commit_in(text: &str) -> Option<&str> {
    let sha = text.trim();
    (sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then_some(sha)
}

/// The recorded commit, when it is one git has and what is checked out descends from it.
pub fn last(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file(root)).ok()?;
    let sha = commit_in(&text)?;
    let ancestor = said(&["git", "merge-base", "--is-ancestor", sha, "HEAD"], root, LIMIT).is_some_and(|(code, _)| code == 0);
    ancestor.then(|| sha.to_owned())
}

/// Records what is checked out as passed: the commit, or why it was not recorded.
pub fn record(root: &Path) -> Result<String, String> {
    // (Files git does not track count: the gate measured them, and no commit holds them.)
    let clean = said_text(&["git", "status", "--porcelain"], root, LIMIT).ok_or("git did not answer")?;
    if !clean.trim().is_empty() {
        return Err("there are uncommitted changes, and a pass is recorded for a commit: commit, then run the gate, and the next one is measured from there".to_owned());
    }
    let head = said_text(&["git", "rev-parse", "HEAD"], root, LIMIT).ok_or("git named no commit")?;
    let sha = commit_in(&head).ok_or("git named no commit")?;
    let path = file(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, format!("{sha}\n")).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(sha.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_is_kept_by_checkout() {
        let (a, b) = (file(Path::new("C:/one/checkout")), file(Path::new("C:/another/checkout")));
        assert_ne!(a, b);
        assert_eq!(a, file(Path::new("c:\\One\\Checkout")), "one checkout, however its path is spelt");
        assert!(a.file_name().unwrap().to_string_lossy().starts_with("passed-"));
    }

    #[test]
    fn only_a_commits_full_name_is_a_record() {
        assert_eq!(commit_in("0123456789abcdef0123456789abcdef01234567\n"), Some("0123456789abcdef0123456789abcdef01234567"));
        assert_eq!(commit_in("origin/main"), None);
        assert_eq!(commit_in("0123456"), None);
    }

    /// A commit git does not have, or one that what is checked out does not descend from, is
    /// not used: the gate then measures from `origin/main`.
    #[test]
    fn a_record_that_is_not_an_ancestor_is_not_used() {
        let root = crate::run::root();
        let unknown = "0123456789abcdef0123456789abcdef01234567";
        let ancestor = said(&["git", "merge-base", "--is-ancestor", unknown, "HEAD"], &root, LIMIT);
        assert!(ancestor.is_none_or(|(code, _)| code != 0), "git does not have that commit");
    }
}

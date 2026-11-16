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

/// `text` as a commit to measure from: one git has, and that what is checked out descends from.
fn usable(root: &Path, text: &str) -> Option<String> {
    let sha = commit_in(text)?;
    let ancestor = said(&["git", "merge-base", "--is-ancestor", sha, "HEAD"], root, LIMIT).is_some_and(|(code, _)| code == 0);
    ancestor.then(|| sha.to_owned())
}

/// The recorded commit, when it is one git has and what is checked out descends from it.
pub fn last(root: &Path) -> Option<String> {
    last_in(root, &file(root))
}

/// [`last`], with the record read from `record` (a test keeps one of its own).
fn last_in(root: &Path, record: &Path) -> Option<String> {
    usable(root, &std::fs::read_to_string(record).ok()?)
}

/// What is checked out, as the gate finds it before it measures anything: the commit, and
/// whether nothing is uncommitted (files git does not track count: the gate measures them,
/// and no commit holds them). `None`: git does not answer here.
pub fn checked_out(root: &Path) -> Option<(String, bool)> {
    let clean = said_text(&["git", "status", "--porcelain"], root, LIMIT)?.trim().is_empty();
    let head = said_text(&["git", "rev-parse", "HEAD"], root, LIMIT)?;
    Some((commit_in(&head)?.to_owned(), clean))
}

/// Records the commit the gate measured as passed, or says why it was not recorded.
/// `measured`: what was checked out when the gate began ([`checked_out`]).
///
/// A pass is a commit's only when the gate ran on that commit from its first step to its last
/// with nothing uncommitted: the same commit, clean, before and after. (Review R1 of M6 part
/// 3: it read the commit at the end. A commit made in the checkout while a small gate ran was
/// recorded as passed, and every later gate was measured from it: its change was never run at
/// the size it needed.)
pub fn record(root: &Path, measured: Option<&(String, bool)>) -> Result<String, String> {
    let uncommitted = "there are uncommitted changes, and a pass is recorded for a commit: commit, then run the gate, and the next one is measured from there";
    let Some((began_at, was_clean)) = measured else { return Err("git did not answer when the gate began".to_owned()) };
    if !was_clean {
        return Err(uncommitted.to_owned());
    }
    let (now, clean) = checked_out(root).ok_or("git did not answer")?;
    if !clean {
        return Err(uncommitted.to_owned());
    }
    if now != *began_at {
        return Err(format!("what is checked out changed while the gate ran (it began at {}, and {} is checked out now): this pass is of neither commit. Run the gate again", &began_at[..10], &now[..10]));
    }
    let path = file(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, format!("{now}\n")).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_is_kept_by_checkout() {
        let (a, b) = (file(Path::new("C:/one/checkout")), file(Path::new("C:/another/checkout")));
        assert_ne!(a, b);
        assert_eq!(a, file(Path::new(r"c:\One\Checkout")), "one checkout, however its path is spelt");
        assert!(a.file_name().unwrap().to_string_lossy().starts_with("passed-"));
    }

    #[test]
    fn only_a_commits_full_name_is_a_record() {
        assert_eq!(commit_in("0123456789abcdef0123456789abcdef01234567\n"), Some("0123456789abcdef0123456789abcdef01234567"));
        assert_eq!(commit_in("origin/main"), None);
        assert_eq!(commit_in("0123456"), None);
    }

    /// A commit git does not have, or one that what is checked out does not descend from, is
    /// not used: the gate then measures from `origin/main`. What is checked out itself is.
    #[test]
    fn a_record_that_is_not_an_ancestor_is_not_used() {
        let root = crate::run::root();
        // (Without git, or outside a checkout, nothing can be said either way.)
        let Some((head, _)) = checked_out(&root) else { return };
        assert_eq!(usable(&root, &format!("{head}\n")), Some(head.clone()), "the commit that is checked out descends from itself");
        assert_eq!(usable(&root, "0123456789abcdef0123456789abcdef01234567"), None, "git does not have that commit");
        assert_eq!(usable(&root, "origin/main"), None, "a name is not a record");
        // The same through the record's file, as the gate reads it: a record of what is
        // checked out is used; one of a commit git does not have, one that is not a commit's
        // name, and no record at all, are not.
        let record = std::env::temp_dir().join(format!("specula-examen-passed-{}.txt", std::process::id()));
        assert_eq!(last_in(&root, &record), None, "no record");
        std::fs::write(&record, format!("{head}
")).unwrap();
        assert_eq!(last_in(&root, &record), Some(head));
        for not_one in ["0123456789abcdef0123456789abcdef01234567
", "origin/main
", ""] {
            std::fs::write(&record, not_one).unwrap();
            assert_eq!(last_in(&root, &record), None, "{not_one:?}");
        }
        let _ = std::fs::remove_file(&record);
    }

    /// A pass is recorded for the commit the gate began at, and only when that commit is still
    /// what is checked out, with nothing uncommitted before or after.
    #[test]
    fn a_pass_is_of_the_commit_the_gate_began_at_or_of_none() {
        let root = crate::run::root();
        let Some((head, clean)) = checked_out(&root) else { return };
        assert!(record(&root, None).unwrap_err().contains("when the gate began"));
        assert!(record(&root, Some(&(head.clone(), false))).unwrap_err().contains("uncommitted"), "it began with changes that no commit holds");
        // It began at another commit than the one checked out now: recorded for neither.
        let other = "0123456789abcdef0123456789abcdef01234567".to_owned();
        let refused = record(&root, Some(&(other, true))).unwrap_err();
        assert!(if clean { refused.contains("changed while the gate ran") } else { refused.contains("uncommitted") }, "{refused}");
    }
}

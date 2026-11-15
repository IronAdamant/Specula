//! Every checkout gives the same bytes: text files are LF in the repository and LF in the working
//! tree, on every system (`.gitattributes`: `* text=auto eol=lf`).
//!
//! Why it is held: tests read tracked files. With `core.autocrlf` a file written on Windows kept
//! its LF until git next touched it, and was CRLF in every fresh checkout. A guard that matched a
//! line with its `\n` passed the gate in the tree it was written in and failed in a fresh one (the
//! push that ended M5). With one ending everywhere, the tree a change is gated in is the tree
//! everyone else gets.
//!
//! Skips cleanly without git.

use std::path::Path;
use std::process::Command;

#[test]
fn text_files_have_one_line_ending_in_every_checkout() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let attributes = std::fs::read_to_string(root.join(".gitattributes")).expect(".gitattributes at the repository's root");
    assert!(attributes.lines().any(|l| l.trim() == "* text=auto eol=lf"), ".gitattributes pins every text file to LF: `* text=auto eol=lf`");
    let listed = match Command::new("git").args(["ls-files", "--eol"]).current_dir(&root).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => {
            eprintln!("skipped: no git to list the tracked files with");
            return;
        }
    };
    // `i/<what the repository holds> w/<the working tree> attr/<attributes> <path>`.
    let mut files = 0;
    let mut bad = Vec::new();
    for line in listed.lines() {
        let mut parts = line.split_whitespace();
        let (Some(index), Some(_tree)) = (parts.next(), parts.next()) else { continue };
        files += 1;
        if index == "i/crlf" || index == "i/mixed" {
            bad.push(line.to_owned());
        }
    }
    assert!(files > 1000, "git listed {files} files: this is not the repository");
    assert!(bad.is_empty(), "text files with CRLF in the repository itself (commit them through .gitattributes: `git add --renormalize .`):\n{}", bad.join("\n"));
}

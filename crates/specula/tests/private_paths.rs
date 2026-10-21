//! The repository is public: no tracked text file may hold a path into someone's home folder, a temp folder
//! under it, or an agent session id. Scripts write reports and transcripts that get committed (perf/, evals/,
//! docs/), and an absolute temp path names the user and, for Claude's scratch folders, the session.
//!
//! Scans every file `git ls-files` lists (skips cleanly without git). Binary files are skipped by extension.
//! `docs/archive/` is exempt (history from before this check, already public). A line that must show such a
//! pattern as test data carries the marker `private-scan: allow`.

use std::path::Path;
use std::process::Command;

const SKIP_EXT: [&str; 16] =
    ["png", "jpg", "jpeg", "spv", "specsave", "glb", "bin", "ttf", "otf", "wav", "ogg", "ico", "dll", "exe", "gif", "zip"];
const ALLOW: &str = "private-scan: allow";

fn is_sep(b: u8) -> bool {
    b == b'\\' || b == b'/'
}

fn skip_seps(line: &[u8], mut i: usize) -> Option<usize> {
    let start = i;
    while i < line.len() && is_sep(line[i]) {
        i += 1;
    }
    (i > start).then_some(i)
}

fn word_at(line: &[u8], i: usize, word: &str) -> bool {
    line.len() >= i + word.len() && line[i..i + word.len()].eq_ignore_ascii_case(word.as_bytes())
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// What private pattern `line` holds, if any.
fn private_marker(line: &[u8]) -> Option<&'static str> {
    let n = line.len();
    for i in 0..n {
        // X:\Users\ (any run of / or \ as separators, so JSON-escaped and URL forms match too)
        if line[i].is_ascii_alphabetic() && i + 1 < n && line[i + 1] == b':' && (i == 0 || !line[i - 1].is_ascii_alphanumeric()) {
            if let Some(j) = skip_seps(line, i + 2) {
                if word_at(line, j, "users") && skip_seps(line, j + 5).is_some() {
                    return Some("a Windows home folder path");
                }
            }
        }
        // /home/<name> and /Users/<name>
        if is_sep(line[i]) && (word_at(line, i + 1, "home") || word_at(line, i + 1, "users")) {
            let j = i + 1 + if word_at(line, i + 1, "home") { 4 } else { 5 };
            if let Some(k) = skip_seps(line, j) {
                if k < n && line[k].is_ascii_alphanumeric() && (i == 0 || !line[i - 1].is_ascii_alphanumeric()) {
                    return Some("a Unix home folder path");
                }
            }
        }
        if word_at(line, i, concat!("app", "data")) {
            return Some("an AppData path");
        }
        // Temp\claude (Claude's scratch folders hold the session id)
        if word_at(line, i, "temp") {
            if let Some(j) = skip_seps(line, i + 4) {
                if word_at(line, j, "claude") {
                    return Some("a Claude temp folder");
                }
            }
        }
        // session_<long id>
        if word_at(line, i, "session_") {
            let id = line[i + 8..].iter().take_while(|b| b.is_ascii_alphanumeric()).count();
            if id >= 16 {
                return Some("a session id");
            }
        }
        // a UUID (8-4-4-4-12 hex), the shape of session and project folder ids
        if i + 36 <= n && (i == 0 || !is_hex(line[i - 1])) {
            let u = &line[i..i + 36];
            let shape = u.iter().enumerate().all(|(k, &b)| if [8, 13, 18, 23].contains(&k) { b == b'-' } else { is_hex(b) });
            if shape && (i + 36 == n || !is_hex(line[i + 36])) {
                return Some("a UUID");
            }
        }
    }
    None
}

#[test]
fn the_marker_finds_what_it_should() {
    let bad: [&[u8]; 8] = [
        br#""file": "C:\\Users\\someone\\x""#,
        br"D:/Users/someone/x",
        br"see /home/someone/project",
        br"/Users/someone/Documents",
        concat!("x\\", "App", "Data", "\\Local").as_bytes(),
        concat!("Temp\\", "claude\\X--folder").as_bytes(),
        concat!("ses", "sion_0123456789ABCDEFabcd").as_bytes(),
        concat!("12345678-90ab-cdef-1234-", "567890abcdef").as_bytes(),
    ];
    for b in bad {
        assert!(private_marker(b).is_some(), "missed: {}", String::from_utf8_lossy(b));
    }
    let good: [&[u8]; 6] = [
        br"C:\VulkanSDK\1.4.363.0\Bin\glslc.exe",
        br"D:\Coding Projects\Specula",
        br"crates/specula/src/users.rs",
        br"the session_id field",
        br"a hash 0123456789abcdef",
        br"<work>\\saves\\proof.specsave",
    ];
    for g in good {
        assert!(private_marker(g).is_none(), "false positive: {}", String::from_utf8_lossy(g));
    }
}

#[test]
fn tracked_files_hold_no_private_paths() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = match Command::new("git").arg("-C").arg(&root).args(["ls-files", "-z"]).output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => {
            println!("skipping: git is not available here");
            return;
        }
    };
    let me = "crates/specula/tests/private_paths.rs";
    let mut found = Vec::new();
    for name in out.split(|&b| b == 0).filter(|n| !n.is_empty()) {
        let name = String::from_utf8_lossy(name).replace('\\', "/");
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if name == me || name.starts_with("docs/archive/") || SKIP_EXT.contains(&ext.as_str()) {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(&name)) else { continue };
        for (k, line) in bytes.split(|&b| b == b'\n').enumerate() {
            if line.windows(ALLOW.len()).any(|w| w == ALLOW.as_bytes()) {
                continue;
            }
            if let Some(what) = private_marker(line) {
                found.push(format!("{name}:{}: {what}", k + 1));
                break;
            }
        }
    }
    assert!(found.is_empty(), "tracked files hold private paths or ids (the repo is public):\n{}", found.join("\n"));
}

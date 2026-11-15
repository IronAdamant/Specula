//! The Rust version this repository is built with (owner, 2026-11-16: one version, named in the
//! repository and in CI, moved on purpose through the gate). `rust-toolchain.toml` names it;
//! rustup reads that file in this checkout, so a developer's default toolchain is never changed
//! for it. The gate fails when the Rust it runs with is another one.

use std::path::Path;
use std::time::Duration;

use crate::run::said_text;

/// A version as numbers: 1.99 is older than 1.100.
pub type Version = (u32, u32, u32);

/// The first `N.N.N` in `text`.
pub fn version_in(text: &str) -> Option<Version> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'.')) {
            let end = i + bytes[i..].iter().take_while(|b| b.is_ascii_digit() || **b == b'.').count();
            // (A version that ends a sentence has a full stop after it.)
            let mut parts = text[i..end].trim_end_matches('.').split('.');
            if let (Some(a), Some(b), Some(c), None) = (parts.next(), parts.next(), parts.next(), parts.next()) {
                if let (Ok(a), Ok(b), Ok(c)) = (a.parse(), b.parse(), c.parse()) {
                    return Some((a, b, c));
                }
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

pub fn dotted(v: Version) -> String {
    format!("{}.{}.{}", v.0, v.1, v.2)
}

/// The version `rust-toolchain.toml` names (`channel = "1.100.0"`).
pub fn pinned(toolchain_toml: &str) -> Option<Version> {
    let line = toolchain_toml.lines().map(str::trim).find(|l| l.starts_with("channel"))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"');
    // A channel such as "stable" names no version: that is not a pin.
    version_in(value).filter(|v| dotted(*v) == value)
}

/// The newest stable Rust that `rustup check` names in its lines for the stable channel (a
/// version installed, or the one an update would bring): never a pinned toolchain's line, a
/// date or a hash.
pub fn newest_stable(check: &str) -> Option<Version> {
    let mut newest = None;
    for line in check.lines().filter(|l| l.starts_with("stable-")) {
        let mut rest = line;
        while let Some(v) = version_in(rest) {
            newest = newest.max(Some(v));
            let at = rest.find(&dotted(v)).map_or(rest.len(), |i| i + dotted(v).len());
            rest = &rest[at..];
        }
    }
    newest
}

/// The version `rustc -V` names (`rustc 1.100.0 (hash date)`): its second word, and only when
/// that word is a version and nothing more. `1.100.0-nightly` and `1.100.0-beta.4` have the
/// pinned number and another compiler, clippy and Cargo (review R1 of this lane, finding 7).
pub fn rustc_version(said: &str) -> Option<Version> {
    let word = said.split_whitespace().nth(1)?;
    version_in(word).filter(|v| dotted(*v) == word)
}

/// What the gate says of the Rust it runs with.
pub struct Rust {
    /// `rustc -V`, as said.
    pub rustc: String,
    pub pinned: Option<Version>,
    /// A newer stable than the pin is out (a note: moving the pin is a change of its own).
    pub newer: Option<Version>,
    /// Why this run's Rust is not the repository's (empty when it is).
    pub failures: Vec<String>,
}

/// Compares what is running with what is pinned. `rustc` and `check` are what `rustc -V` and
/// `rustup check` said ("" when they did not answer).
pub fn judge(toolchain_toml: Option<&str>, rustc: &str, check: &str) -> Rust {
    let pin = toolchain_toml.and_then(pinned);
    let mine = rustc_version(rustc);
    let mut failures = Vec::new();
    match (pin, mine) {
        (None, _) => failures.push("rust: rust-toolchain.toml names no version (channel = \"N.N.N\"): the repository pins one".to_owned()),
        (Some(pin), None) => failures.push(format!(
            "rust: rustc -V said {:?}, which names no released version, and the repository pins {} (a nightly or a beta of that number is another Rust)",
            rustc.trim(),
            dotted(pin)
        )),
        (Some(pin), Some(mine)) if pin != mine => failures.push(format!(
            "rust: this run used {} and the repository pins {}: run cargo in this checkout with no RUSTUP_TOOLCHAIN set and no +toolchain (rustup reads rust-toolchain.toml; `rustup toolchain install {}` installs it beside the default)",
            dotted(mine),
            dotted(pin),
            dotted(pin)
        )),
        _ => {}
    }
    let newer = newest_stable(check).filter(|n| pin.is_some_and(|p| *n > p));
    Rust { rustc: rustc.trim().to_owned(), pinned: pin, newer, failures }
}

/// `judge`, of this checkout and this machine.
pub fn in_use(root: &Path) -> Rust {
    let toml = std::fs::read_to_string(root.join("rust-toolchain.toml")).ok();
    let rustc = said_text(&["rustc", "-V"], root, Duration::from_secs(60)).unwrap_or_default();
    // (Where rustup can't say, offline or absent, there is no note; the pin is what is checked.)
    let check = said_text(&["rustup", "check"], root, Duration::from_secs(45)).unwrap_or_default();
    judge(toml.as_deref(), &rustc, &check)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = "[toolchain]\nchannel = \"1.100.0\"\ncomponents = [\"clippy\"]\n";

    #[test]
    fn versions_are_numbers() {
        assert_eq!(version_in("rustc 1.100.0 (0ef1bc386 2026-11-09)"), Some((1, 100, 0)));
        assert_eq!(version_in("rustc 1.99.1 (58cf8f9c3 2026-10-13)"), Some((1, 99, 1)));
        assert!((1, 99, 1) < (1, 100, 0), "1.99 is older than 1.100");
        assert_eq!(version_in("no version here: 2026-11-09 and 1.2"), None);
        assert_eq!(version_in("x1.2.3"), None, "a version starts a word");
        assert_eq!(version_in("1.2.3.4 then 5.6.7"), Some((5, 6, 7)), "four numbers are not a version");
    }

    #[test]
    fn the_pin_is_a_version_and_never_a_channel() {
        assert_eq!(pinned(TOML), Some((1, 100, 0)));
        assert_eq!(pinned("[toolchain]\nchannel = \"stable\"\n"), None);
        assert_eq!(pinned("[toolchain]\nchannel = \"1.100\"\n"), None, "all three numbers");
        assert_eq!(pinned("[toolchain]\nchannel = \"1.100.0-x86_64-pc-windows-gnu\"\n"), None, "no host: every machine reads this file");
        assert_eq!(pinned(""), None);
    }

    /// The newest stable, from rustup's own lines (both spellings it has used).
    #[test]
    fn the_newest_stable_is_read_from_the_stable_channels_lines() {
        let check = "stable-x86_64-pc-windows-gnu - update available: 1.99.1 (58cf8f9c3 2026-10-13) -> 1.100.0 (0ef1bc386 2026-11-09)\n\
            stable-x86_64-pc-windows-msvc - Update available : 1.89.0 (29483883e 2025-08-04) -> 1.100.0 (0ef1bc386 2026-11-09)\n\
            1.101.0-x86_64-pc-windows-gnu - up to date: 1.101.0 (aaaaaaaaa 2026-12-21)\n\
            rustup - update available : 1.29.1 -> 1.29.3\n";
        assert_eq!(newest_stable(check), Some((1, 100, 0)));
        assert_eq!(newest_stable("stable-x86_64-unknown-linux-gnu - Up to date : 1.100.0 (0ef1bc386 2026-11-09)\n"), Some((1, 100, 0)));
        assert_eq!(newest_stable(""), None);
        assert_eq!(newest_stable("error: could not download file\n"), None);
    }

    #[test]
    fn another_rust_than_the_pinned_one_fails() {
        let same = judge(Some(TOML), "rustc 1.100.0 (0ef1bc386 2026-11-09)\n", "");
        assert!(same.failures.is_empty() && same.newer.is_none() && same.pinned == Some((1, 100, 0)), "{:?}", same.failures);
        let older = judge(Some(TOML), "rustc 1.99.1 (58cf8f9c3 2026-10-13)", "");
        assert!(older.failures.len() == 1 && older.failures[0].contains("used 1.99.1") && older.failures[0].contains("pins 1.100.0"), "{:?}", older.failures);
        let newer = judge(Some(TOML), "rustc 1.101.0 (aaaaaaaaa 2026-12-21)", "");
        assert_eq!(newer.failures.len(), 1, "a newer one than the pin is another one too");
        assert_eq!(judge(None, "rustc 1.100.0", "").failures.len(), 1, "no pin is a failure");
        assert_eq!(judge(Some(TOML), "", "").failures.len(), 1);
        // The pinned number on another Rust.
        for other in ["rustc 1.100.0-nightly (0ef1bc386 2026-11-09)", "rustc 1.100.0-beta.4 (0ef1bc386 2026-11-09)", "rustc 1.100.0-dev", "rustc v1.100.0", "1.100.0"] {
            let r = judge(Some(TOML), other, "");
            assert!(r.failures.len() == 1 && r.failures[0].contains("pins 1.100.0"), "{other}: {:?}", r.failures);
        }
        assert_eq!(rustc_version("rustc 1.100.0 (0ef1bc386 2026-11-09)"), Some((1, 100, 0)));
        assert_eq!(rustc_version("rustc 1.100.0\n"), Some((1, 100, 0)));
    }

    #[test]
    fn a_newer_stable_is_a_note_and_not_a_failure() {
        let r = judge(Some(TOML), "rustc 1.100.0", "stable-x86_64-pc-windows-gnu - update available: 1.100.0 (x 2026-11-09) -> 1.101.0 (y 2026-12-21)\n");
        assert!(r.failures.is_empty() && r.newer == Some((1, 101, 0)));
        let r = judge(Some(TOML), "rustc 1.100.0", "stable-x86_64-pc-windows-gnu - up to date: 1.100.0 (x 2026-11-09)\n");
        assert!(r.failures.is_empty() && r.newer.is_none());
    }

    /// The pin is said in three places, and they agree: the toolchain file, the workspace's
    /// declared minimum, and every version CI installs.
    #[test]
    fn the_repository_names_one_rust_version() {
        let root = crate::run::root();
        let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
        let pin = pinned(&read("rust-toolchain.toml")).expect("rust-toolchain.toml pins a version");
        let manifest = read("Cargo.toml");
        let declared = manifest.lines().find_map(|l| l.trim().strip_prefix("rust-version = ")).expect("the workspace declares rust-version");
        assert_eq!(declared.trim_matches('"'), dotted(pin), "Cargo.toml's rust-version is the pinned version");
        let ci = read(".github/workflows/ci.yml");
        assert!(!ci.contains("@stable") && !ci.contains("install stable"), "CI installs the pinned version, never whatever stable is newest (the scheduled job in newest.yml does that)");
        let installs: Vec<&str> = ci.lines().filter(|l| l.contains("rustup toolchain install")).collect();
        assert!(installs.len() >= 3, "CI's jobs install their Rust with rustup: {installs:?}");
        for line in installs {
            assert_eq!(version_in(line), Some(pin), "CI installs another Rust than the pinned one: {}", line.trim());
        }
    }
}

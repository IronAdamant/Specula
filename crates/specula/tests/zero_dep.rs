//! Specula ships zero third-party crates: every package in Cargo.lock must be a workspace member.

use std::path::Path;

#[test]
fn cargo_lock_has_only_workspace_packages() {
    let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    let text = std::fs::read_to_string(&lock).expect("Cargo.lock (run cargo build first)");
    let mut name = "";
    let mut offenders = Vec::new();
    for line in text.lines() {
        if let Some(n) = line.strip_prefix("name = ") {
            name = n.trim_matches('"');
        } else if line.starts_with("source = ") {
            offenders.push(name.to_owned());
        }
    }
    assert!(offenders.is_empty(), "third-party crates in Cargo.lock: {offenders:?}");
}

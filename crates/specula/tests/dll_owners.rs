//! AGENTS.md rule 2: each DLL is declared (`#[link(name = "X", kind = "raw-dylib")]`) in exactly
//! one crate's sources, and the rule's list of owners says which. On windows-gnu a second crate's
//! raw-dylib imports from the same DLL are silently dropped (a crash at call time), so the list
//! is what a new import is checked against: it must match the code (review of m5-text: `imm32`
//! was declared in specula-platform but missing from the list).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// DLL → the crates whose `src` declares it.
fn declared() -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for dir in ["crates", "games"] {
        let Ok(crates) = std::fs::read_dir(root().join(dir)) else { continue };
        for c in crates.flatten() {
            let name = c.file_name().to_string_lossy().into_owned();
            let mut files = Vec::new();
            rust_files(&c.path().join("src"), &mut files);
            for f in files {
                let text = std::fs::read_to_string(&f).unwrap_or_default();
                for line in text.lines().map(str::trim).filter(|l| l.starts_with("#[link(") && l.contains("raw-dylib")) {
                    let Some(dll) = line.split("name = \"").nth(1).and_then(|r| r.split('"').next()) else { continue };
                    let owners = out.entry(dll.to_owned()).or_default();
                    if !owners.contains(&name) {
                        owners.push(name.clone());
                    }
                }
            }
        }
    }
    out
}

/// Rule 2's owners list: clauses `` `a`, `b` and `c` → `crate` `` up to "Runtime-loaded".
fn listed() -> BTreeMap<String, String> {
    let agents = std::fs::read_to_string(root().join("AGENTS.md")).expect("AGENTS.md");
    let start = agents.find("2. **OS/driver imports").expect("rule 2");
    let rule = &agents[start..];
    let rule = &rule[rule.find("exactly one crate").expect("the one-crate sentence")..rule.find("Runtime-loaded").expect("the runtime-loaded sentence")];
    let rule = rule.replace(['\n', '\r'], " ");
    let ticks = |s: &str| s.split('`').skip(1).step_by(2).map(str::to_owned).collect::<Vec<_>>();
    let mut out = BTreeMap::new();
    for clause in rule.split(';') {
        let Some((dlls, owner)) = clause.split_once('→') else { continue };
        let owner = ticks(owner).into_iter().next().expect("an owning crate in backticks");
        for dll in ticks(dlls) {
            out.insert(dll, owner.clone());
        }
    }
    out
}

#[test]
fn every_raw_dylib_has_one_owner_and_agents_md_lists_it() {
    let code = declared();
    assert!(code.contains_key("imm32") && code.contains_key("kernel32"), "the scan found the declarations: {code:?}");
    for (dll, owners) in &code {
        assert_eq!(owners.len(), 1, "{dll} is declared with raw-dylib in more than one crate: {owners:?} (AGENTS.md rule 2)");
    }
    let code: BTreeMap<String, String> = code.into_iter().map(|(d, o)| (d, o[0].clone())).collect();
    assert_eq!(listed(), code, "AGENTS.md rule 2's list of DLL owners (left) must match the raw-dylib declarations in crates/*/src (right)");
}

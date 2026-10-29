//! The lint rules every workspace member signs up to, checked so a new crate can't skip them
//! (docs/design/DETERMINISM.md, AGENTS.md rules 8 and 10):
//!
//! - every member inherits `[workspace.lints]` (`[lints] workspace = true`): SAFETY comments on
//!   unsafe blocks, `# Safety` docs on unsafe functions, unsafe operations in unsafe blocks;
//! - every member without FFI forbids unsafe code in each crate root;
//! - every **simulation** crate has the determinism `clippy.toml` (no HashMap/HashSet, platform
//!   maths, wall-clock time or threads), identical to the reference copy;
//! - every member is classified as one or the other: a new crate fails here until it is added to
//!   [`SIMULATION`] or [`NOT_SIMULATION`].

use std::path::{Path, PathBuf};

/// Crates whose code runs in (or decides) the simulation: they carry the determinism clippy.toml.
const SIMULATION: [&str; 13] = [
    "crates/specula-physics",
    // Cosmetic, but reproducible: captures repeat bit for bit on every platform (docs/design/VFX.md).
    "crates/specula-scintilla",
    // Rendering data, but reproducible: the same scene bakes to the same bytes everywhere (docs/design/GI.md).
    "crates/specula-lux",
    "crates/specula-anim",
    "crates/specula-lex",
    "crates/specula-balance",
    "crates/specula-sdk",
    "crates/specula-via",
    "crates/specula-mens",
    "crates/specula-locus",
    // Recipes build the meshes ragdolls fit their bones to and the hulls bodies collide with
    // (docs/design/PROJECT_DATA.md), so mesh building feeds the simulation.
    "crates/specula-model",
    "games/sepulcra",
    "games/ambulo",
];

/// Crates outside the simulation (rendering, platform, tools, presentation; or the engine, whose
/// simulated parts are checked by review and the replay tests rather than one crate-wide config).
const NOT_SIMULATION: [&str; 14] = [
    "crates/specula-sys",
    "crates/specula-core",
    "crates/specula-gltf",
    "crates/specula-platform",
    "crates/specula-input",
    "crates/specula-gpu",
    "crates/specula-protocol",
    "crates/specula-text",
    "crates/specula-audio",
    "crates/specula-ui",
    "crates/specula-logic",
    // The asset studio measures renders and recipes for review; nothing in it runs in the simulation.
    "crates/specula-studio",
    "crates/specula",
    "games/sepulcra-app",
];

/// Crates with FFI (or other unsafe code, each block with a SAFETY comment): the rest forbid it.
/// `specula-core` denies it outside its counting allocator; module crates (`ambulo`) deny it
/// because `export_module!` allows its one exported function.
const UNSAFE_ALLOWED: [&str; 8] = [
    "crates/specula-sys",
    "crates/specula-platform",
    "crates/specula-gpu",
    "crates/specula-audio",
    "crates/specula-logic",
    "crates/specula-sdk",
    "crates/specula-core",
    "games/ambulo",
];

/// The reference determinism clippy.toml.
const REFERENCE: &str = "crates/specula-physics/clippy.toml";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())).replace("\r\n", "\n")
}

/// The workspace members, as listed in the root Cargo.toml.
fn members() -> Vec<String> {
    let root = read(&repo().join("Cargo.toml"));
    let list = root.split("members = [").nth(1).and_then(|s| s.split(']').next()).expect("a members list");
    list.lines().map(str::trim).filter_map(|l| l.strip_prefix('"').and_then(|l| l.split('"').next())).map(str::to_owned).collect()
}

/// A crate's root source files (lib.rs, main.rs, src/bin/*.rs).
fn roots(member: &str) -> Vec<PathBuf> {
    let src = repo().join(member).join("src");
    let mut out: Vec<PathBuf> = ["lib.rs", "main.rs"].iter().map(|f| src.join(f)).filter(|p| p.exists()).collect();
    if let Ok(bins) = std::fs::read_dir(src.join("bin")) {
        out.extend(bins.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "rs")));
    }
    out
}

#[test]
fn every_member_is_classified() {
    let members = members();
    assert!(members.len() >= 20, "found the members list: {members:?}");
    let unknown: Vec<&String> = members.iter().filter(|m| !SIMULATION.contains(&m.as_str()) && !NOT_SIMULATION.contains(&m.as_str())).collect();
    assert!(
        unknown.is_empty(),
        "new workspace members {unknown:?}: add each to SIMULATION (and give it a copy of {REFERENCE}) or NOT_SIMULATION in this test"
    );
    let gone: Vec<&str> = SIMULATION.iter().chain(&NOT_SIMULATION).copied().filter(|c| !members.iter().any(|m| m == c)).collect();
    assert!(gone.is_empty(), "listed here but not workspace members any more: {gone:?}");
}

#[test]
fn every_member_inherits_the_workspace_lints() {
    let root = read(&repo().join("Cargo.toml"));
    for lint in ["unsafe_op_in_unsafe_fn", "undocumented_unsafe_blocks", "missing_safety_doc"] {
        assert!(root.contains(&format!("{lint} = \"deny\"")), "[workspace.lints] denies {lint}");
    }
    let missing: Vec<String> = members()
        .into_iter()
        .filter(|m| {
            let toml = read(&repo().join(m).join("Cargo.toml"));
            !toml.split("[lints]").nth(1).is_some_and(|rest| rest.trim_start().starts_with("workspace = true"))
        })
        .collect();
    assert!(missing.is_empty(), "add `[lints]\\nworkspace = true` to the Cargo.toml of {missing:?}");
}

#[test]
fn crates_without_ffi_forbid_unsafe_code() {
    let mut missing = Vec::new();
    for m in members().iter().filter(|m| !UNSAFE_ALLOWED.contains(&m.as_str())) {
        for root in roots(m) {
            if !read(&root).lines().any(|l| l.trim() == "#![forbid(unsafe_code)]") {
                missing.push(root.strip_prefix(repo()).unwrap_or(&root).display().to_string());
            }
        }
    }
    assert!(missing.is_empty(), "add #![forbid(unsafe_code)] to {missing:?} (or, for a crate with FFI, list it in UNSAFE_ALLOWED)");
}

#[test]
fn simulation_crates_carry_the_determinism_lints() {
    let reference = read(&repo().join(REFERENCE));
    for needle in ["std::collections::HashMap", "f32::sin", "f64::round", "std::time::Instant::now", "std::thread::spawn"] {
        assert!(reference.contains(needle), "{REFERENCE} bans {needle}");
    }
    let mut differ = Vec::new();
    for m in SIMULATION {
        let p = repo().join(m).join("clippy.toml");
        if !p.exists() || read(&p) != reference {
            differ.push(m);
        }
    }
    assert!(differ.is_empty(), "these simulation crates need a clippy.toml identical to {REFERENCE}: {differ:?}");
}

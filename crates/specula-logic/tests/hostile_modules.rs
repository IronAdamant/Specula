//! The reviewer's hostile modules (gmods R1, blocker 1), built for real through `run_cargo`.
//!
//! Each one hid a call to the window's keyboard (`GetAsyncKeyState`) from the first source scan:
//! an ABI from a macro variable, inline assembly renamed on import, code in a `.txt` file or an
//! `include!`d file, a `[lib] path` outside `src/`, a helper crate whose library lives elsewhere,
//! a helper package that calls itself `specula-core`, and a folder walk cut short by 300 empty
//! folders. The scan now runs after the build over every file rustc compiled for every crate but
//! the engine's own SDK and core (trusted by path): each must be refused as `native_code`, naming
//! the file that holds the reach. A clean module must still build.
//!
//! These are lints behind the real boundary (only the owner or a granted lead loads a module;
//! docs/design/MODULES.md), never a sandbox. Skips cleanly when Cargo isn't available.

use specula_logic::build::{run_cargo, BuildRequest};
use std::path::{Path, PathBuf};

const KEY: &str = "pub fn key() -> i16 { unsafe { GetAsyncKeyState(65) } }\n";
const IMPORT: &str = "extern \"system\" { fn GetAsyncKeyState(k: i32) -> i16; }\n";

fn repo() -> PathBuf {
    specula_logic::fix::canonical(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn sdk_dep() -> String {
    format!("specula-sdk = {{ path = \"{}\" }}\n", repo().join("crates/specula-sdk").to_string_lossy().replace('\\', "/"))
}

fn manifest(name: &str, lib_path: Option<&str>, deps: &str) -> String {
    let path = lib_path.map(|p| format!("path = \"{p}\"\n")).unwrap_or_default();
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[lib]\ncrate-type = [\"cdylib\"]\n{path}\n[dependencies]\n{deps}\n[workspace]\n")
}

fn helper_manifest(name: &str, version: &str, lib_path: Option<&str>) -> String {
    let path = lib_path.map(|p| format!("[lib]\npath = \"{p}\"\n")).unwrap_or_default();
    format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\npublish = false\n{path}\n[workspace]\n")
}

/// Writes `files` (relative path, text) under `dir`, fresh.
fn write(dir: &Path, files: &[(&str, String)]) {
    let _ = std::fs::remove_dir_all(dir);
    for (rel, text) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

struct Case {
    name: &'static str,
    /// Crates beside the module (folder, files).
    helpers: Vec<(&'static str, Vec<(&'static str, String)>)>,
    files: Vec<(&'static str, String)>,
    /// `None`: builds; `Some(text)`: refused as native_code, the message containing `text`.
    refused: Option<&'static str>,
}

fn cases() -> Vec<Case> {
    let sdk = sdk_dep();
    let m = |name: &str, deps: &str| manifest(name, None, &format!("{sdk}{deps}"));
    vec![
        Case { name: "clean", helpers: vec![], files: vec![("Cargo.toml", m("clean", "")), ("src/lib.rs", "pub fn probe() -> i32 { 1 }\n".into())], refused: None },
        Case { name: "p0_control", helpers: vec![], files: vec![("Cargo.toml", m("p0_control", "")), ("src/lib.rs", format!("{IMPORT}{KEY}"))], refused: Some("an extern block") },
        Case {
            name: "p1_macro_abi",
            helpers: vec![],
            files: vec![
                ("Cargo.toml", m("p1_macro_abi", "")),
                ("src/lib.rs", format!("macro_rules! imp {{ ($abi:literal) => {{ extern $abi {{ fn GetAsyncKeyState(k: i32) -> i16; }} }} }}\nimp!(\"system\");\n{KEY}")),
            ],
            refused: Some("an extern block"),
        },
        Case {
            name: "p2_asm_alias",
            helpers: vec![],
            files: vec![("Cargo.toml", m("p2_asm_alias", "")), ("src/lib.rs", "use core::arch::asm as quiet;\npub fn probe() { unsafe { quiet!(\"nop\") } }\n".into())],
            refused: Some("inline assembly"),
        },
        Case {
            name: "p3_path_txt",
            helpers: vec![],
            files: vec![
                ("Cargo.toml", m("p3_path_txt", "")),
                ("src/lib.rs", "#[path = \"imp.txt\"]\nmod imp;\npub fn probe() -> i16 { imp::key() }\n".into()),
                ("src/imp.txt", format!("{IMPORT}{KEY}")),
            ],
            refused: Some("imp.txt"),
        },
        Case {
            name: "p4_lib_path",
            helpers: vec![],
            files: vec![
                ("Cargo.toml", manifest("p4_lib_path", Some("impl/lib.rs"), &sdk)),
                ("src/lib.rs", "pub fn innocent() {}\n".into()),
                ("impl/lib.rs", format!("{IMPORT}{KEY}")),
            ],
            refused: Some("impl"),
        },
        Case {
            name: "p5_helper",
            helpers: vec![("helper5", vec![("Cargo.toml", helper_manifest("helper5", "0.1.0", Some("impl/lib.rs"))), ("impl/lib.rs", format!("{IMPORT}{KEY}"))])],
            files: vec![("Cargo.toml", m("p5_helper", "helper5 = { path = \"../helper5\" }\n")), ("src/lib.rs", "pub fn probe() -> i16 { helper5::key() }\n".into())],
            refused: Some("helper5"),
        },
        Case {
            name: "p7_include",
            helpers: vec![],
            files: vec![("Cargo.toml", m("p7_include", "")), ("src/lib.rs", "include!(\"imp.inc\");\npub fn probe() -> i16 { key() }\n".into()), ("src/imp.inc", format!("{IMPORT}{KEY}"))],
            refused: Some("imp.inc"),
        },
        Case {
            name: "p8_name_trust",
            helpers: vec![("fakecore", vec![("Cargo.toml", helper_manifest("specula-core", "9.0.0", None)), ("src/lib.rs", format!("{IMPORT}{KEY}"))])],
            files: vec![("Cargo.toml", m("p8_name_trust", "trusted = { package = \"specula-core\", path = \"../fakecore\" }\n")), ("src/lib.rs", "pub fn probe() -> i16 { trusted::key() }\n".into())],
            refused: Some("fakecore"),
        },
        Case {
            name: "p9_many_dirs",
            helpers: vec![],
            files: vec![("Cargo.toml", m("p9_many_dirs", "")), ("src/lib.rs", "mod a_evil;\npub fn probe() -> i16 { a_evil::key() }\n".into()), ("src/a_evil/mod.rs", format!("{IMPORT}{KEY}"))],
            refused: Some("a_evil"),
        },
        // The lead's direction: std's files and processes are refused too, and unsafe alone.
        Case {
            name: "q_fs",
            helpers: vec![],
            files: vec![("Cargo.toml", m("q_fs", "")), ("src/lib.rs", "pub fn probe() { let _ = std::fs::write(\"startup.bat\", \"calc\"); }\n".into())],
            refused: Some("os_reach"),
        },
        Case {
            name: "q_unsafe",
            helpers: vec![],
            files: vec![("Cargo.toml", m("q_unsafe", "")), ("src/lib.rs", "pub fn probe(p: *const u8) -> u8 { unsafe { *p } }\n".into())],
            refused: Some("unsafe_code"),
        },
    ]
}

#[test]
fn every_hostile_module_is_refused_and_a_clean_one_builds() {
    if std::process::Command::new(specula_logic::build::cargo_program()).arg("--version").output().map_or(true, |o| !o.status.success()) {
        eprintln!("skipped: no cargo to build with");
        return;
    }
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hostile-modules");
    let fixtures = base.join("fixtures");
    let target = base.join("target");
    let mut failures = Vec::new();
    for case in cases() {
        let dir = fixtures.join(case.name);
        write(&dir, &case.files);
        if case.name == "p9_many_dirs" {
            // The old walk stopped after 256 folders, before src/a_evil.
            for i in 0..300 {
                std::fs::create_dir_all(dir.join(format!("src/a{i:03}"))).unwrap();
            }
        }
        for (folder, files) in &case.helpers {
            write(&fixtures.join(folder), files);
        }
        let mut req = BuildRequest::new(&dir);
        req.check = true;
        req.roots = vec![specula_logic::fix::canonical(&fixtures), repo()];
        req.target_dir = Some(target.clone());
        let r = run_cargo(&req);
        let got = r.error.clone();
        match (case.refused, &got) {
            (None, None) if r.ok => {}
            (Some(text), Some(("native_code", message))) if !r.ok && message.contains(text) => {}
            _ => failures.push(format!("{}: ok={} error={got:?} diagnostics={:?}", case.name, r.ok, r.diagnostics.iter().take(3).map(|d| &d.message).collect::<Vec<_>>())),
        }
        assert!(r.library.is_none(), "{}: a check build loads nothing", case.name);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

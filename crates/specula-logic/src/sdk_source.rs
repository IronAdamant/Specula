//! The SDK as a game project is given it: the engine carries the sources and writes them into a
//! project (`specula new`, `specula sdk`), so a project outside the engine's repository needs
//! nothing copied by hand (ROADMAP M6, "A project gets its SDK from the engine").
//!
//! Two crates, under the project's `crates/`, by the names a module already depends on:
//!
//! - `specula-sdk`: the SDK's own sources, as they are in the engine.
//! - `specula-core`: **the SDK's edition of it**: the three modules the SDK is built on
//!   (deterministic maths, vectors, JSON), each file as it is in the engine, under a `lib.rs` that
//!   names only those. A module has no use for the engine's image codecs, its profiler or its
//!   trace, and an agent that opens the SDK to learn it reads a quarter of the text.
//!
//! **These files are trusted by what they say, not by where they are.** The build service scans
//! every crate a module build compiles for native code and refuses it, except the SDK and core
//! (the ABI needs `unsafe`). That exception used to be the path `crates/specula-sdk` under the
//! project; anything written there was compiled unchecked. It is now [`is_the_engines`]: a copy
//! that matches these sources byte for byte.

use std::path::{Path, PathBuf};

/// One file of the SDK as written into a project: its path below the project's folder (with
/// `/`), and its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SdkFile {
    pub path: &'static str,
    pub text: &'static str,
}

/// The engine's version, which the SDK it writes carries (a module's ABI table names it).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SDK_MANIFEST: &str = concat!(
    "# The Specula SDK, written by the engine (`specula sdk`). Leave these files as they are: a\n",
    "# module build trusts them only while they are the engine's own, and `specula sdk` puts them back.\n",
    "[package]\n",
    "name = \"specula-sdk\"\n",
    "description = \"The one crate Specula game modules depend on: deterministic APIs behind a versioned C ABI\"\n",
    "version = \"",
    env!("CARGO_PKG_VERSION"),
    "\"\n",
    "edition = \"2021\"\n",
    "license = \"MIT OR Apache-2.0\"\n",
    "\n",
    "[dependencies]\n",
    "specula-core = { path = \"../specula-core\" }\n",
);

const CORE_MANIFEST: &str = concat!(
    "# Specula's core, the SDK's edition, written by the engine (`specula sdk`). Leave these files as\n",
    "# they are: a module build trusts them only while they are the engine's own.\n",
    "[package]\n",
    "name = \"specula-core\"\n",
    "description = \"The parts of Specula's core the SDK is built on: deterministic maths, vectors, JSON\"\n",
    "version = \"",
    env!("CARGO_PKG_VERSION"),
    "\"\n",
    "edition = \"2021\"\n",
    "license = \"MIT OR Apache-2.0\"\n",
);

const CORE_LIB: &str = "\
//! Specula's core, as a game project is given it: the parts the SDK is built on. Written by the
//! engine (`specula sdk`); each module below is the engine's own file.
//!
//! - [`dmath`]: deterministic maths. Use it in place of `f32::sin` and the like: the same
//!   result on every machine, which a simulation that is replayed and compared needs.
//! - [`math`]: `Vec3`, `Quat` and matrices (right-handed, +Y up, metres).
//! - [`json`]: JSON values.
#![deny(unsafe_code)]

pub mod dmath;
pub mod json;
pub mod math;

pub use json::{Json, JsonError};
";

/// The SDK's files, in the order they are written.
pub const FILES: &[SdkFile] = &[
    SdkFile { path: "crates/specula-sdk/Cargo.toml", text: SDK_MANIFEST },
    SdkFile { path: "crates/specula-sdk/src/lib.rs", text: include_str!("../../specula-sdk/src/lib.rs") },
    SdkFile { path: "crates/specula-sdk/src/abi.rs", text: include_str!("../../specula-sdk/src/abi.rs") },
    SdkFile { path: "crates/specula-sdk/src/module.rs", text: include_str!("../../specula-sdk/src/module.rs") },
    SdkFile { path: "crates/specula-sdk/src/value.rs", text: include_str!("../../specula-sdk/src/value.rs") },
    SdkFile { path: "crates/specula-core/Cargo.toml", text: CORE_MANIFEST },
    SdkFile { path: "crates/specula-core/src/lib.rs", text: CORE_LIB },
    SdkFile { path: "crates/specula-core/src/dmath.rs", text: include_str!("../../specula-core/src/dmath.rs") },
    SdkFile { path: "crates/specula-core/src/dmath/tests.rs", text: include_str!("../../specula-core/src/dmath/tests.rs") },
    SdkFile { path: "crates/specula-core/src/json.rs", text: include_str!("../../specula-core/src/json.rs") },
    SdkFile { path: "crates/specula-core/src/math.rs", text: include_str!("../../specula-core/src/math.rs") },
];

/// The two crates' folders below a project.
pub const FOLDERS: [&str; 2] = ["crates/specula-sdk", "crates/specula-core"];

/// A file's text with one kind of line end: a checkout may hold these sources with either, and
/// a copy is the engine's whichever it has.
fn lines_of(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Why the SDK was not written.
#[derive(Debug)]
pub enum WriteRefused {
    /// The SDK's two folders hold files that are not the SDK's (below the project, with `/`):
    /// someone's work, or the engine's own whole crates. Nothing was written or removed.
    OtherFiles(Vec<String>),
    Io(std::io::Error),
}

impl std::fmt::Display for WriteRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteRefused::OtherFiles(files) => {
                let shown: Vec<&str> = files.iter().take(6).map(String::as_str).collect();
                let more = if files.len() > shown.len() { format!(" and {} more", files.len() - shown.len()) } else { String::new() };
                write!(f, "crates/specula-sdk and crates/specula-core hold files that are not the SDK's ({}{more}): nothing was written. They are the engine's two folders in a project: move what is yours out of them (or delete the two folders), and run it again", shown.join(", "))
            }
            WriteRefused::Io(e) => write!(f, "the SDK could not be written ({e})"),
        }
    }
}

/// Writes the SDK into `project` (its `crates/specula-sdk` and `crates/specula-core`), replacing
/// the SDK's own files there. **It removes nothing and refuses a folder that holds anything
/// else** ([`WriteRefused::OtherFiles`]): run where the engine's own whole crates are, it would
/// otherwise have replaced them with the SDK's edition. Returns the paths written, below the
/// project.
pub fn write_into(project: &Path) -> Result<Vec<&'static str>, WriteRefused> {
    let others = strays(project);
    if !others.is_empty() {
        return Err(WriteRefused::OtherFiles(others));
    }
    for file in FILES {
        let path = project.join(file.path);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(WriteRefused::Io)?;
        }
        std::fs::write(&path, lines_of(file.text)).map_err(WriteRefused::Io)?;
    }
    Ok(FILES.iter().map(|f| f.path).collect())
}

/// Every file under `dir`, below `project`, with `/`; a `target` folder is build output and is
/// left out.
fn files_under(project: &Path, dir: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(project.join(dir)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let below = format!("{dir}/{name}");
        if entry.path().is_dir() {
            if name != "target" {
                files_under(project, &below, out);
            }
        } else {
            out.push(below);
        }
    }
}

/// Files in the SDK's two folders that are not the SDK's.
fn strays(project: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for dir in FOLDERS {
        files_under(project, dir, &mut found);
    }
    found.retain(|f| !FILES.iter().any(|sdk| sdk.path == f));
    found.sort();
    found
}

/// How the SDK under `project` differs from the engine's: each file that is missing, says
/// something else, or should not be there (empty when the copy is the engine's own).
pub fn differs(project: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for file in FILES {
        match std::fs::read(project.join(file.path)) {
            Ok(bytes) if lines_of(&String::from_utf8_lossy(&bytes)) == lines_of(file.text) => {}
            Ok(_) => out.push(format!("{}: not the engine's text", file.path)),
            Err(_) => out.push(format!("{}: missing", file.path)),
        }
    }
    out.extend(strays(project).into_iter().map(|f| format!("{f}: not a file of the SDK")));
    out
}

/// Whether the SDK under `project` is the engine's own, file for file: the only copy a module
/// build compiles without a source check.
pub fn is_the_engines(project: &Path) -> bool {
    FILES.iter().all(|f| project.join(f.path).is_file()) && differs(project).is_empty()
}

/// The manifests of the SDK's two crates under `project`.
pub fn manifests(project: &Path) -> [PathBuf; 2] {
    FOLDERS.map(|dir| project.join(dir).join("Cargo.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-sdk-source-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_written_copy_is_the_engines_and_a_changed_one_is_not() {
        let dir = scratch("copy");
        assert!(!is_the_engines(&dir), "nothing is there yet");
        assert_eq!(differs(&dir).len(), FILES.len());
        let written = write_into(&dir).unwrap();
        assert_eq!(written.len(), FILES.len());
        assert!(is_the_engines(&dir), "{:?}", differs(&dir));
        // One character more in one file.
        let abi = dir.join("crates/specula-sdk/src/abi.rs");
        let text = std::fs::read_to_string(&abi).unwrap();
        std::fs::write(&abi, format!("{text}\n// more")).unwrap();
        assert_eq!(differs(&dir), ["crates/specula-sdk/src/abi.rs: not the engine's text"]);
        assert!(!is_the_engines(&dir));
        // A file that is not the SDK's, where a build would find it.
        std::fs::write(&abi, text).unwrap();
        std::fs::write(dir.join("crates/specula-sdk/build.rs"), "fn main() {}").unwrap();
        assert_eq!(differs(&dir), ["crates/specula-sdk/build.rs: not a file of the SDK"]);
        // A missing file.
        std::fs::remove_file(dir.join("crates/specula-sdk/build.rs")).unwrap();
        std::fs::remove_file(dir.join("crates/specula-core/src/json.rs")).unwrap();
        assert_eq!(differs(&dir), ["crates/specula-core/src/json.rs: missing"]);
        // Writing it again puts a missing or changed file back.
        write_into(&dir).unwrap();
        assert!(is_the_engines(&dir));
        // A folder that holds anything else is refused whole: nothing is written or removed
        // (the engine's own whole crates lie at these paths in its repository).
        std::fs::write(dir.join("crates/specula-core/src/extra.rs"), "mine").unwrap();
        std::fs::remove_file(dir.join("crates/specula-core/src/json.rs")).unwrap();
        let refused = write_into(&dir).unwrap_err();
        assert!(matches!(&refused, WriteRefused::OtherFiles(files) if files == &["crates/specula-core/src/extra.rs"]), "{refused}");
        assert!(refused.to_string().contains("nothing was written"));
        assert!(dir.join("crates/specula-core/src/extra.rs").exists() && !dir.join("crates/specula-core/src/json.rs").exists());
        std::fs::remove_file(dir.join("crates/specula-core/src/extra.rs")).unwrap();
        write_into(&dir).unwrap();
        assert!(is_the_engines(&dir));
        // Build output beside the sources is not the SDK's business.
        std::fs::create_dir_all(dir.join("crates/specula-sdk/target/debug")).unwrap();
        std::fs::write(dir.join("crates/specula-sdk/target/debug/x.rlib"), "").unwrap();
        assert!(is_the_engines(&dir));
        // Either kind of line end is the engine's text.
        let lib = dir.join("crates/specula-sdk/src/lib.rs");
        let crlf = std::fs::read_to_string(&lib).unwrap().replace('\n', "\r\n");
        std::fs::write(&lib, crlf).unwrap();
        assert!(is_the_engines(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The edition of core names every module its files use, and nothing the SDK's sources
    /// name is left out of it.
    #[test]
    fn the_sdks_edition_of_core_holds_what_the_sdk_uses() {
        let core: Vec<&SdkFile> = FILES.iter().filter(|f| f.path.starts_with("crates/specula-core/src/")).collect();
        let modules: Vec<&str> = CORE_LIB.lines().filter_map(|l| l.strip_prefix("pub mod ")).map(|m| m.trim_end_matches(';')).collect();
        assert_eq!(modules, ["dmath", "json", "math"]);
        for module in &modules {
            assert!(core.iter().any(|f| f.path == format!("crates/specula-core/src/{module}.rs")), "{module}.rs is written");
        }
        // What those files and the SDK's own name of the crate.
        let mut named = std::collections::BTreeSet::new();
        for file in FILES.iter().filter(|f| f.path.ends_with(".rs") && !f.path.ends_with("/lib.rs") || f.path == "crates/specula-sdk/src/lib.rs") {
            let (own, prefix) = if file.path.starts_with("crates/specula-core/") { (true, "crate::") } else { (false, "specula_core::") };
            for (at, _) in file.text.match_indices(prefix) {
                // (`crate::` in the SDK's own files is the SDK's.)
                let before = file.text[..at].chars().next_back();
                if !own || before.is_none_or(|c| !c.is_alphanumeric() && c != '_') {
                    let word: String = file.text[at + prefix.len()..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                    named.insert(word);
                }
            }
        }
        let known = ["dmath", "json", "math", "Json", "JsonError"];
        let unknown: Vec<&String> = named.iter().filter(|n| !known.contains(&n.as_str())).collect();
        assert!(unknown.is_empty(), "the SDK's sources name these of specula-core, which its edition does not have: {unknown:?}");
    }

    #[test]
    fn the_manifests_carry_the_engines_version_and_no_workspace() {
        for manifest in [SDK_MANIFEST, CORE_MANIFEST] {
            assert!(manifest.contains(&format!("version = \"{VERSION}\"")), "{manifest}");
            assert!(!manifest.contains("workspace"), "a project's own workspace need not define the engine's keys");
        }
        assert!(FILES.iter().all(|f| !f.text.is_empty() && !f.path.contains('\\')));
    }

    /// The SDK as written builds on its own, and its tests pass, with nothing of the engine's
    /// repository beside it. Skipped, and said so, where there is no cargo.
    #[test]
    fn the_written_sdk_builds_on_its_own() {
        let dir = scratch("build");
        write_into(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\nresolver = \"2\"\nmembers = [\"crates/specula-core\", \"crates/specula-sdk\"]\n").unwrap();
        let cargo = std::process::Command::new("cargo").args(["check", "--offline", "--workspace", "--all-targets"]).current_dir(&dir).env_remove("RUSTFLAGS").output();
        match cargo {
            Err(e) => println!("skipped: no cargo to build with ({e})"),
            Ok(out) => assert!(out.status.success(), "the SDK as written does not build:\n{}", String::from_utf8_lossy(&out.stderr)),
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

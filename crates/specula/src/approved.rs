//! Which module sources an owner or a granted lead last built: what `--run` may load
//! (docs/design/MODULES.md, "Who may load a module"; review R1 of M6 part 3, finding S1).
//!
//! `--run` has the engine build and load the modules a project's game file names, with no
//! agent asking. A builder may only propose a module: the owner or a lead reviews its files
//! and builds it. But a builder can write a module's sources and the game file through the
//! project tools, and whatever was on disk at the next `--run` was built and loaded as the
//! operator's own: the review was skipped by restarting.
//!
//! So every build that an owner or a granted lead asked for, and that loaded, leaves a record:
//! the digest of the module's folder (`specula_logic::digest`: every file's path, length and
//! bytes). `--run` builds a module only while its folder still has that digest. Sources
//! changed since, by anyone and any way (an agent's `project_write`, an undo, an editor, a
//! pull), are not loaded at start: the game says so, and an owner or lead builds them
//! (`logic_build`), which is the review. `--run-as-is` is the operator saying that what is on
//! disk now is theirs to run, whatever built it.
//!
//! **The record is the operator's, not the project's.** It is kept in the per-user settings
//! folder (`--settings DIR`, by default the user's own), by the project's folder. No tool of
//! the engine writes there for an agent, and a project someone sends carries no record: its
//! code runs only when the person says `--run-as-is`, or builds it as its owner. Without a
//! settings folder (`--settings off`) there is no record, and `--run` loads nothing.
//!
//! Reads and writes are small files, on the caller's thread or in the command line's own
//! process: never on the engine thread.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use specula_core::Json;

const FORMAT: &str = "specula.approved.v1";
/// The folder under the settings folder.
pub const FOLDER: &str = "approved";
/// The largest record read (a project has at most 32 modules).
const MAX_BYTES: u64 = 64 * 1024;

static RECORDS: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Where this engine keeps its records: the settings folder it was started with, or `None`
/// (`--settings off`). Set once, at start.
pub fn set_settings(settings: Option<&Path>) {
    let _ = RECORDS.set(settings.map(|s| s.join(FOLDER)));
}

/// This engine's records folder (`None`: it keeps none).
pub fn folder() -> Option<PathBuf> {
    RECORDS.get().cloned().flatten()
}

/// A path as the record names it: resolved, with `/`, and in one case on Windows.
fn key(path: &Path) -> String {
    let text = specula_logic::fix::canonical(path).to_string_lossy().replace('\\', "/");
    let text = text.trim_end_matches('/').to_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// The module's folder below its project, with `/` (`modules/gate`), or `None` when it is not
/// below it.
fn module_key(project: &Path, module: &Path) -> Option<String> {
    let (p, m) = (key(project), key(module));
    m.strip_prefix(&p).and_then(|rest| rest.strip_prefix('/')).filter(|rest| !rest.is_empty()).map(str::to_owned)
}

fn file(records: &Path, project: &Path) -> PathBuf {
    records.join(format!("{:016x}.json", specula_core::hash::hash_str(&key(project))))
}

fn read(records: &Path, project: &Path) -> Vec<(String, String)> {
    let path = file(records, project);
    if !std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_BYTES) {
        return Vec::new();
    }
    let Some(json) = std::fs::read_to_string(&path).ok().and_then(|text| Json::parse(&text).ok()) else { return Vec::new() };
    // (Two folders may share a hash: a record is its project's only when it names it.)
    if json.get("format").and_then(Json::as_str) != Some(FORMAT) || json.get("project").and_then(Json::as_str) != Some(key(project).as_str()) {
        return Vec::new();
    }
    let hex = |d: &str| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit());
    json.get("modules").and_then(Json::as_object).map(|m| m.iter().filter_map(|(k, v)| v.as_str().filter(|d| hex(d)).map(|d| (k.clone(), d.to_owned()))).collect()).unwrap_or_default()
}

/// The digest `module` (a folder below `project`) had when an owner or a granted lead last
/// built it here, from the records in `records`.
pub fn approved_in(records: &Path, project: &Path, module: &Path) -> Option<String> {
    let name = module_key(project, module)?;
    read(records, project).into_iter().find(|(k, _)| *k == name).map(|(_, digest)| digest)
}

/// Records that `module` (a folder below `project`) was built, with this digest of its
/// folder, by someone who may load native code.
pub fn record_in(records: &Path, project: &Path, module: &Path, digest: &str) -> Result<(), String> {
    let name = module_key(project, module).ok_or_else(|| format!("{} is not below {}", module.display(), project.display()))?;
    let mut modules = read(records, project);
    modules.retain(|(k, _)| *k != name);
    modules.push((name, digest.to_owned()));
    modules.sort();
    // (At most a project's 32 modules, and folders that are gone drop out.)
    modules.retain(|(k, _)| k.split('/').fold(project.to_path_buf(), |p, part| p.join(part)).is_dir());
    modules.truncate(64);
    let json = Json::obj([("format", Json::from(FORMAT)), ("project", Json::from(key(project).as_str())), ("modules", Json::Obj(modules.into_iter().map(|(k, d)| (k, Json::from(d.as_str()))).collect()))]);
    let book = crate::disk::book();
    book.make_dir(crate::disk::Category::Settings, records, crate::disk::Need::Essential).map_err(|e| e.to_string())?;
    book.write_synced(crate::disk::Category::Settings, "approved", &file(records, project), (json.to_pretty() + "\n").as_bytes(), crate::disk::Need::Essential).map_err(|e| e.to_string())
}

/// [`approved_in`] with this engine's records.
pub fn approved(project: &Path, module: &Path) -> Option<String> {
    approved_in(&folder()?, project, module)
}

/// [`record_in`] with this engine's records (nothing to do when it keeps none).
pub fn record(project: &Path, module: &Path, digest: &str) {
    let Some(records) = folder() else { return };
    if let Err(why) = record_in(&records, project, module, digest) {
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "approved.not_recorded", module = module.to_string_lossy().as_ref(), why = why.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-approved-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_build_is_recorded_by_its_project_and_its_module() {
        let dir = scratch("record");
        let (records, crypt, frost) = (dir.join("settings").join(FOLDER), dir.join("crypt"), dir.join("frost"));
        for module in [crypt.join("modules").join("gate"), crypt.join("modules").join("walls"), frost.join("modules").join("gate")] {
            std::fs::create_dir_all(module).unwrap();
        }
        let (a, b) = ("a".repeat(64), "b".repeat(64));
        assert_eq!(approved_in(&records, &crypt, &crypt.join("modules").join("gate")), None, "nothing was ever built");
        record_in(&records, &crypt, &crypt.join("modules").join("gate"), &a).unwrap();
        record_in(&records, &crypt, &crypt.join("modules").join("walls"), &b).unwrap();
        assert_eq!(approved_in(&records, &crypt, &crypt.join("modules").join("gate")).as_deref(), Some(a.as_str()));
        assert_eq!(approved_in(&records, &crypt, &crypt.join("modules").join("walls")).as_deref(), Some(b.as_str()));
        // Another project's module of the same name is another module.
        assert_eq!(approved_in(&records, &frost, &frost.join("modules").join("gate")), None);
        // A later build replaces the digest; a module outside the project has no record.
        record_in(&records, &crypt, &crypt.join("modules").join("gate"), &b).unwrap();
        assert_eq!(approved_in(&records, &crypt, &crypt.join("modules").join("gate")).as_deref(), Some(b.as_str()));
        assert!(record_in(&records, &crypt, &frost.join("modules").join("gate"), &a).is_err());
        assert_eq!(approved_in(&records, &crypt, &crypt), None);
        // A module whose folder is gone drops out at the next record.
        std::fs::remove_dir_all(crypt.join("modules").join("walls")).unwrap();
        record_in(&records, &crypt, &crypt.join("modules").join("gate"), &a).unwrap();
        assert_eq!(approved_in(&records, &crypt, &crypt.join("modules").join("walls")), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A record that is not one, or is another project's, approves nothing.
    #[test]
    fn a_record_that_is_not_this_projects_approves_nothing() {
        let dir = scratch("forged");
        let (records, crypt) = (dir.join(FOLDER), dir.join("crypt"));
        let module = crypt.join("modules").join("gate");
        std::fs::create_dir_all(&module).unwrap();
        std::fs::create_dir_all(&records).unwrap();
        let digest = "c".repeat(64);
        for text in [
            "not json".to_owned(),
            format!(r#"{{"format": "specula.approved.v1", "project": "/somewhere/else", "modules": {{"modules/gate": "{digest}"}}}}"#),
            format!(r#"{{"format": "other", "project": "{}", "modules": {{"modules/gate": "{digest}"}}}}"#, key(&crypt)),
            format!(r#"{{"format": "specula.approved.v1", "project": "{}", "modules": {{"modules/gate": "not a digest"}}}}"#, key(&crypt)),
        ] {
            std::fs::write(file(&records, &crypt), &text).unwrap();
            assert_eq!(approved_in(&records, &crypt, &module), None, "{text}");
        }
        std::fs::write(file(&records, &crypt), format!(r#"{{"format": "specula.approved.v1", "project": "{}", "modules": {{"modules/gate": "{digest}"}}}}"#, key(&crypt))).unwrap();
        assert_eq!(approved_in(&records, &crypt, &module), Some(digest));
        let _ = std::fs::remove_dir_all(dir);
    }
}

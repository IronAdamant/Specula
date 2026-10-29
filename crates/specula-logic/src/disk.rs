//! The engine's disk budget, seen from the module host: every file this crate writes or deletes
//! (loaded library copies, fixes written into a module's sources, a restored `Cargo.lock`) goes
//! through here. This crate can't depend on the engine, so the engine installs its accounting
//! point as a hook at start ([`set_hook`]); without one (tests, tools) writes simply happen.
//! `crates/specula/tests/disk_guard.rs` fails when this crate writes a file anywhere else.

use std::path::Path;
use std::sync::OnceLock;

/// What the module host writes, for the budget's categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Library copies and anything else under a module's build target.
    Modules,
    /// A module's sources and its workspace's `Cargo.lock` (the project's own files).
    Project,
}

/// Admits `bytes` more of `kind` (`essential`: a repair written even below the free-space floor);
/// an error says why it was refused.
pub type Admit = fn(kind: Kind, bytes: u64, essential: bool) -> Result<(), String>;
/// Records bytes written (or, negative, removed) and files added (or removed).
pub type Wrote = fn(kind: Kind, bytes: i64, files: i64);

static HOOK: OnceLock<(Admit, Wrote)> = OnceLock::new();

/// Installs the engine's accounting point (once).
pub fn set_hook(admit: Admit, wrote: Wrote) {
    let _ = HOOK.set((admit, wrote));
}

fn len(path: &Path) -> Option<u64> {
    std::fs::symlink_metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len())
}

/// Writes `data` to `path` (replacing it), admitted and accounted.
pub fn write(kind: Kind, path: &Path, data: &[u8], essential: bool) -> std::io::Result<()> {
    let old = len(path);
    if let Some((admit, _)) = HOOK.get() {
        admit(kind, (data.len() as u64).saturating_sub(old.unwrap_or(0)), essential).map_err(std::io::Error::other)?;
    }
    std::fs::write(path, data)?;
    if let Some((_, wrote)) = HOOK.get() {
        wrote(kind, data.len() as i64 - old.unwrap_or(0) as i64, i64::from(old.is_none()));
    }
    Ok(())
}

/// Makes `dir` and its parents, admitted like a small write.
pub fn make_dir(kind: Kind, dir: &Path) -> std::io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    if let Some((admit, _)) = HOOK.get() {
        admit(kind, 4096, false).map_err(std::io::Error::other)?;
    }
    std::fs::create_dir_all(dir)
}

/// Removes one file, accounted.
pub fn remove_file(kind: Kind, path: &Path) -> std::io::Result<()> {
    let old = len(path);
    std::fs::remove_file(path)?;
    if let (Some((_, wrote)), Some(n)) = (HOOK.get(), old) {
        wrote(kind, -(n as i64), -1);
    }
    Ok(())
}

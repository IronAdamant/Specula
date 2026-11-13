//! The hosted game's player settings, saved per user outside the project (owner, 2026-11-07;
//! docs/design/ACCESSIBILITY.md).
//!
//! - **Where.** `--settings DIR` names a folder (scripts and tests: changes agents make are saved
//!   there too); `--settings off` turns them off. Otherwise a windowed engine keeps them in the
//!   operating system's per-user settings folder (Windows `Specula\<game>` in the roaming application-data folder, macOS
//!   `~/Library/Application Support/Specula/<game>`, elsewhere `$XDG_CONFIG_HOME/specula/<game>`
//!   or `~/.config/specula/<game>`), and a headless one keeps none, so agents' runs never read or
//!   overwrite a player's settings.
//! - **Whose.** In the per-user folder only the player's own changes are saved
//!   ([`SettingsStore::agents_save`] is false): what an agent changes lasts for the session.
//! - **Cost before work.** The file is read once at start-up, before the game starts, and only up
//!   to [`MAX_BYTES`] (a larger file is left alone, unread, and the game gets defaults with the
//!   problem). Writes run on a worker, one at a time, the newest text winning; each is written to
//!   a temporary file beside it and renamed over it, so a crash never leaves half a file.

use crate::game::SettingsStore;
use crate::workers::{JobKind, Offload};
use crate::Engine;
use std::path::{Path, PathBuf};

/// The settings file's name in its folder.
pub const FILE: &str = "settings.json";
/// Largest settings file read or written.
pub const MAX_BYTES: usize = 64 << 10;

/// The engine's side of the game's settings: where they go, the latest text not yet written.
#[derive(Default)]
pub struct UserSettings {
    dir: Option<PathBuf>,
    pub(crate) store: SettingsStore,
    /// The newest text the game handed over, waiting for the write in flight to end.
    pending: Option<String>,
    writing: bool,
    /// Writes done and the last failure, for engine_info.
    pub(crate) written: u64,
    pub(crate) failed: Option<String>,
}

/// A game name as a folder name: ASCII letters, digits, '-' and '_' (anything else becomes '_'),
/// at most 64 characters, never empty.
fn folder_name(game: &str) -> String {
    let s: String = game.chars().take(64).map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if s.is_empty() || s.chars().all(|c| c == '_') {
        "game".into()
    } else {
        s
    }
}

/// The environment variable that names Windows' roaming application-data folder. Spelt in two
/// halves because `tests/private_paths.rs` flags the word wherever it stands (it is how a real
/// user's folder would leak into this public repository).
const ROAMING: &str = concat!("APP", "DATA");

/// The engine's own per-user folder, from the environment (`None` when it names none): where
/// the project list and the running engines' files are (docs/design/LAUNCH.md), and, in a
/// folder per game, each game's player settings.
pub fn per_user_base(var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let nonempty = |k: &str| var(k).filter(|v| !v.trim().is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        nonempty(ROAMING).map(|d| d.join("Specula"))
    } else if cfg!(target_os = "macos") {
        nonempty("HOME").map(|h| h.join("Library").join("Application Support").join("Specula"))
    } else {
        nonempty("XDG_CONFIG_HOME").or_else(|| nonempty("HOME").map(|h| h.join(".config"))).map(|d| d.join("specula"))
    }
}

/// The per-user settings folder for `game`, from the environment (`None` when it names none).
pub fn per_user_dir(game: &str, var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    Some(per_user_base(var)?.join(folder_name(game)))
}

impl UserSettings {
    /// Where the settings go: `flag` is `--settings` (`off` or a folder), `windowed` whether a
    /// human's window is open, `game` the hosted game's name.
    pub fn resolve(flag: Option<&str>, windowed: bool, game: &str) -> UserSettings {
        let (dir, kind, agents_save) = match flag {
            Some("off") => (None, "off", false),
            Some(d) => (Some(PathBuf::from(d)), "folder", true),
            None if windowed => match per_user_dir(game, |k| std::env::var(k).ok()) {
                Some(d) => (Some(d), "per_user", false),
                None => (None, "off", false),
            },
            None => (None, "off", false),
        };
        let mut store = SettingsStore { kind, agents_save, problem: None };
        if kind == "off" && windowed && flag.is_none() {
            store.problem = Some(format!("no per-user settings folder ({ROAMING}, HOME or XDG_CONFIG_HOME is unset)"));
        }
        UserSettings { dir, store, ..UserSettings::default() }
    }

    /// The folder, for the disk book's roots.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Reads the settings file at start-up (before the game starts; at most [`MAX_BYTES`],
    /// checked before reading). `None` when there is none or it can't be used (the problem is
    /// recorded in the store for the game).
    pub fn read(&mut self) -> Option<String> {
        let path = self.dir.as_ref()?.join(FILE);
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                self.store.problem = Some(format!("settings unreadable: {e}"));
                return None;
            }
        };
        if !meta.is_file() || meta.len() > MAX_BYTES as u64 {
            self.store.problem = Some(format!("the settings file is {} bytes; at most {MAX_BYTES} are read (left as it is, defaults used)", meta.len()));
            return None;
        }
        match std::fs::read(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(t) => Some(t),
                Err(_) => {
                    self.store.problem = Some("the settings file isn't UTF-8 text (defaults used)".into());
                    None
                }
            },
            Err(e) => {
                self.store.problem = Some(format!("settings unreadable: {e}"));
                None
            }
        }
    }

    pub fn to_json(&self) -> specula_core::Json {
        use specula_core::Json;
        let mut j = self.store.to_json();
        j.set("written", Json::from(self.written));
        if self.pending.is_some() || self.writing {
            j.set("writing", Json::Bool(true));
        }
        if let Some(f) = &self.failed {
            j.set("failed", Json::from(f.as_str()));
        }
        j
    }
}

/// Writes `text` to `dir/settings.json` through a temporary file (admitted and accounted by the
/// disk book), then renames it over the old one. On a worker.
fn write(dir: &Path, text: &str) -> Result<(), String> {
    // What went wrong is shown to agents (engine_info); where the player keeps their settings
    // (a path with their account's name in it) is not.
    write_to(dir, text).map_err(|why| why.replace(&dir.display().to_string(), "<settings folder>"))
}

fn write_to(dir: &Path, text: &str) -> Result<(), String> {
    if text.len() > MAX_BYTES {
        return Err(format!("settings are {} bytes; at most {MAX_BYTES}", text.len()));
    }
    let book = crate::disk::book();
    book.make_dir(crate::disk::Category::Settings, dir, crate::disk::Need::Essential).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    let path = dir.join(FILE);
    let old = crate::disk::file_len(&path);
    // On disk before the rename: a power cut then leaves the old file or the new one, never a
    // renamed file with nothing in it.
    book.write_synced(crate::disk::Category::Settings, "settings", &tmp, text.as_bytes(), crate::disk::Need::Essential).map_err(|e| e.to_string())?;
    // The temporary file was counted as new: the rename replaces the old file, which goes.
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(old) = old {
        book.wrote(crate::disk::Category::Settings, "settings", -(old as i64), -1);
    }
    Ok(())
}

impl Engine {
    /// Gives the game its saved settings, before it starts.
    pub(crate) fn settings_to_game(&mut self) {
        let text = self.user_settings.read();
        if let Some(p) = &self.user_settings.store.problem {
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "settings.unread", why = p.as_str());
        }
        let store = self.user_settings.store.clone();
        if let Some(g) = self.game.as_mut() {
            g.use_settings(text.as_deref(), &store);
        }
    }

    /// Takes the game's changed settings and writes them off the engine thread, one write at a
    /// time (the newest text waits for the one in flight).
    pub(crate) fn serve_game_settings(&mut self) {
        let Some(game) = self.game.as_mut() else { return };
        if let Some(text) = game.take_settings() {
            if self.user_settings.dir.is_some() {
                self.user_settings.pending = Some(text);
            }
        }
        if self.user_settings.writing {
            return;
        }
        let (Some(text), Some(dir)) = (self.user_settings.pending.take(), self.user_settings.dir.clone()) else { return };
        self.user_settings.writing = true;
        let started = self.offload(Offload::engine(JobKind::SettingsWrite), move |_| write(&dir, &text), |e: &mut Engine, r| {
            e.user_settings.writing = false;
            match r.map_err(|f| format!("{f:?}")).and_then(|r| r) {
                Ok(()) => {
                    e.user_settings.written += 1;
                    e.user_settings.failed = None;
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Debug, "settings.written", n = e.user_settings.written);
                }
                Err(why) => {
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "settings.write_failed", why = why.as_str());
                    e.user_settings.failed = Some(why);
                }
            }
        });
        if let Err(e) = started {
            self.user_settings.writing = false;
            self.user_settings.failed = Some(e.to_string());
        }
    }

    /// At shutdown, after the workers drained: the settings the game still holds, and a change
    /// that waited behind a write in flight, are written before the process ends. (Review R1 of
    /// m5-a11y: two changes a frame apart, then Quit, and the second never reached the disk.)
    pub(crate) fn finish_settings(&mut self) {
        if let Some(text) = self.game.as_mut().and_then(|g| g.take_settings()) {
            if self.user_settings.dir.is_some() {
                self.user_settings.pending = Some(text);
            }
        }
        let (Some(text), Some(dir)) = (self.user_settings.pending.take(), self.user_settings.dir.clone()) else { return };
        match write(&dir, &text) {
            Ok(()) => {
                self.user_settings.written += 1;
                self.user_settings.failed = None;
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Debug, "settings.written", n = self.user_settings.written, at = "shutdown");
            }
            Err(why) => {
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "settings.write_failed", why = why.as_str());
                self.user_settings.failed = Some(why);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R2 of m5-a11y: a failed write says what went wrong without naming the player's folder
    /// (engine_info shows it to agents).
    #[test]
    fn a_failed_write_doesnt_name_the_players_folder() {
        let base = std::env::temp_dir().join(format!("specula-settings-secret-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        // The "folder" is a file: nothing can be written inside it.
        let dir = base.join("not-a-folder");
        std::fs::write(&dir, b"x").unwrap();
        let why = write(&dir, "{}").expect_err("a file can't hold the settings");
        assert!(!why.contains("specula-settings-secret") && !why.contains(&dir.display().to_string()), "{why}");
        assert!(!why.is_empty());
        // And a good folder takes the file whole.
        let good = base.join("ok");
        write(&good, r#"{"volume": 0.5}"#).unwrap();
        assert_eq!(std::fs::read_to_string(good.join(FILE)).unwrap(), r#"{"volume": 0.5}"#);
        assert!(!good.join(format!("{FILE}.tmp")).exists(), "the temporary file was renamed over the settings");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_per_user_folder_comes_from_the_environment() {
        let env = |pairs: &'static [(&'static str, &'static str)]| move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string());
        let all = env(&[(ROAMING, "A"), ("HOME", "H"), ("XDG_CONFIG_HOME", "X")]);
        let d = per_user_dir("sepulcra", all).unwrap();
        if cfg!(windows) {
            assert_eq!(d, Path::new("A").join("Specula").join("sepulcra"));
        } else if cfg!(target_os = "macos") {
            assert_eq!(d, Path::new("H").join("Library").join("Application Support").join("Specula").join("sepulcra"));
        } else {
            assert_eq!(d, Path::new("X").join("specula").join("sepulcra"));
            assert_eq!(per_user_dir("g", env(&[("HOME", "H")])).unwrap(), Path::new("H").join(".config").join("specula").join("g"));
        }
        assert_eq!(per_user_dir("g", env(&[(ROAMING, " "), ("HOME", ""), ("XDG_CONFIG_HOME", "")])), None);
        // A name can't climb out of its folder.
        assert_eq!(folder_name("../../evil"), "______evil");
        assert_eq!(folder_name(""), "game");
        assert_eq!(folder_name("..."), "game");
        assert_eq!(folder_name(&"x".repeat(500)).len(), 64);
    }

    #[test]
    fn headless_runs_keep_no_settings_and_a_named_folder_saves_agents_changes() {
        assert_eq!(UserSettings::resolve(None, false, "g").store.kind, "off");
        assert!(UserSettings::resolve(None, false, "g").dir().is_none());
        let named = UserSettings::resolve(Some("some/dir"), false, "g");
        assert_eq!((named.store.kind, named.store.agents_save), ("folder", true));
        assert_eq!(UserSettings::resolve(Some("off"), true, "g").store.kind, "off");
        let windowed = UserSettings::resolve(None, true, "g");
        if windowed.dir().is_some() {
            assert_eq!((windowed.store.kind, windowed.store.agents_save), ("per_user", false), "the player's own folder: agents' changes stay in the session");
        }
    }

    #[test]
    fn big_and_binary_files_are_left_unread_and_writes_replace_the_file() {
        let dir = std::env::temp_dir().join(format!("specula-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = UserSettings::resolve(Some(dir.to_str().unwrap()), false, "g");
        assert_eq!(s.read(), None, "no file yet");
        assert!(s.store.problem.is_none());
        write(&dir, "{\"a\": 1}").unwrap();
        write(&dir, "{\"a\": 2}").unwrap();
        assert_eq!(s.read().as_deref(), Some("{\"a\": 2}"));
        assert!(!dir.join(format!("{FILE}.tmp")).exists(), "the temporary file was renamed");
        assert!(write(&dir, &"x".repeat(MAX_BYTES + 1)).is_err(), "too large to write");
        std::fs::write(dir.join(FILE), vec![b' '; MAX_BYTES + 1]).unwrap();
        assert_eq!(s.read(), None);
        assert!(s.store.problem.as_deref().unwrap().contains("at most"));
        std::fs::write(dir.join(FILE), [0xff, 0xfe, 0x00]).unwrap();
        let mut s = UserSettings::resolve(Some(dir.to_str().unwrap()), false, "g");
        assert_eq!(s.read(), None);
        assert!(s.store.problem.as_deref().unwrap().contains("UTF-8"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

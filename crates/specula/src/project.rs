//! Project files: the text files a game is made of (model recipes, UI screens and themes, sound
//! recipes, style files, module sources), mirrored into a versioned store so they get what world
//! entities have: claims, versions, the journal, undo, the change feed and attribution. Design:
//! docs/design/PROJECT_DATA.md.
//!
//! - **The store** is held by the [`crate::world::World`] (`World::files`), keyed by project
//!   path (`assets/ui/hud.json`). Every change is a [`crate::world::Change::File`] at the
//!   journal path `/project/<path>` carrying the whole text before and after (shared `Arc<str>`),
//!   so the journal undoes and replays it exactly. Files are not world state: the world hash and
//!   world files leave them out.
//! - **Mounts** map a name to a folder: `assets` (the engine's assets root) and
//!   `modules/<name>` (a module crate). Only files of known kinds ([`FileKind`]) are mirrored.
//! - **The disk** ([`Disk`]): the engine writes mounted files atomically after each commit and
//!   notices edits made outside it (a watcher thread stats them once a second); those are
//!   imported as journaled changes by [`DISK_AGENT`].
//! - **Binary files** (PNG images, glTF/GLB models, WAV audio; [`FileKind::is_binary`]) are held
//!   as small pointer texts naming their bytes in the content store ([`crate::blobs`]): the
//!   journal, conflict help and undo work on the pointer, and the disk sync writes the bytes.

use crate::world::{validate_name, WorldError};
use specula_core::hash::StableHasher;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// The journal and claim root of project files: `/project/<path>`.
pub const ROOT: &str = "project";
/// Journal ids of file changes: this bit plus a hash of the path (entity ids never get this high;
/// cameras use the bit below).
pub const FILE_ID_BASE: u64 = 1 << 63;
/// The writer of changes found on disk (edits made outside the engine); no agent may take it.
pub const DISK_AGENT: &str = "disk";
/// The assets mount's name.
pub const ASSETS: &str = "assets";
/// Module mounts are `modules/<crate name>`.
pub const MODULES: &str = "modules";
/// The project-root mount (an engine opened on a project): its name is empty, so its files are
/// named as they lie in the project folder: `game.json`, and `notes/<id>.md`
/// (docs/design/FIRST_HOUR.md, slices 3 and 6).
pub const ROOT_MOUNT: &str = "";
/// The folder of a project's notes, and its game file.
pub const NOTES: &str = "notes";
pub const GAME_FILE: &str = "game.json";
/// Largest file mirrored or written (UTF-8 text).
pub const MAX_FILE_BYTES: usize = 1 << 20;
/// Files in the store, and their text in all.
pub const MAX_FILES: usize = 4096;
pub const MAX_STORE_BYTES: usize = 32 << 20;
/// Taste state's own budget (galleries, suggestions, boards, onboarding), apart from the above:
/// 512 galleries and their boards fit; filling it never stops a project_write.
pub const MAX_TASTE_FILES: usize = 1024;
pub const MAX_TASTE_BYTES: usize = 64 << 20;
/// Module crates mounted at once.
pub const MAX_MODULE_MOUNTS: usize = 16;
/// Scans go at most this deep and look at most this many directory entries per mount.
const SCAN_DEPTH: usize = 8;
/// Why a write into a module folder waits while that module is built (retried like any failed
/// write, so it lands once the build has loaded).
const BUILDING: &str = "module_building: this module is being built; the file is written when the build ends (the sources a build checks are the ones it compiles)";
const SCAN_ENTRIES: usize = 20_000;
/// Folders scans never enter (build output, the engine's own output).
const SKIP_DIRS: [&str; 7] = ["target", "worlds", "saves", "captures", "node_modules", "specula-modules", "perf"];
/// How often the watcher stats the mirrored files, and looks for new ones.
const WATCH_EVERY: Duration = Duration::from_secs(1);
const RESCAN_EVERY: u32 = 5;
/// Disk conflicts remembered for `project_list`.
const MAX_CONFLICTS: usize = 32;

/// What a file is, which decides how it is validated and what reloads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileKind {
    Recipe,
    Style,
    StyleRules,
    UiScreen,
    UiTheme,
    Sound,
    /// `*.json` under `music/`: a music piece (pattern sequencer, `specula_audio::music`).
    Music,
    AnimSet,
    Rust,
    Cargo,
    Json,
    /// `*.json` under `taste/` at the assets root: galleries, suggestions, boards, onboarding
    /// (docs/design/GALLERIES.md). Reserved data: only the taste tools and the window write it
    /// (`Rule::Taste`); private by default (raw picks and notes).
    Taste,
    /// `*.json` under `style/` at the assets root: approved priors (tracked, not private).
    StyleData,
    /// `*.md` under `style/`: the style configurator's notes, once approved.
    StyleNotes,
    /// `*.json` in a folder named `timelines`: timelines, sequencer tracks and camera rails
    /// (docs/design/TIMELINE.md).
    Timeline,
    /// `*.json` in a folder named `strings`: string tables, one per locale
    /// (docs/design/LOCALISATION.md).
    Strings,
    /// `<id>.md` in a project's `notes` folder: what the project remembers (`specula_memoria`;
    /// the `memory_*` tools read and write them, and a person any editor).
    Note,
    /// `game.json` at a project's root: the game file (`specula_ludus::GameFile`), which the
    /// project shell follows while the engine runs.
    Game,
    /// Binary files (see [`FileKind::is_binary`]).
    Png,
    Glb,
    Gltf,
    Wav,
    /// Ogg Vorbis audio (music, long sounds), streamed by the engine's audio service.
    Ogg,
}

impl FileKind {
    pub const ALL: [FileKind; 23] = [
        FileKind::Recipe,
        FileKind::Style,
        FileKind::StyleRules,
        FileKind::UiScreen,
        FileKind::UiTheme,
        FileKind::Sound,
        FileKind::Music,
        FileKind::AnimSet,
        FileKind::Rust,
        FileKind::Cargo,
        FileKind::Json,
        FileKind::Taste,
        FileKind::StyleData,
        FileKind::StyleNotes,
        FileKind::Timeline,
        FileKind::Strings,
        FileKind::Note,
        FileKind::Game,
        FileKind::Png,
        FileKind::Glb,
        FileKind::Gltf,
        FileKind::Wav,
        FileKind::Ogg,
    ];

    pub fn name(self) -> &'static str {
        match self {
            FileKind::Recipe => "recipe",
            FileKind::Style => "style",
            FileKind::StyleRules => "style_rules",
            FileKind::UiScreen => "ui_screen",
            FileKind::UiTheme => "ui_theme",
            FileKind::Sound => "sound",
            FileKind::Music => "music",
            FileKind::AnimSet => "anim_set",
            FileKind::Rust => "rust",
            FileKind::Cargo => "cargo",
            FileKind::Json => "json",
            FileKind::Taste => "taste",
            FileKind::StyleData => "style_data",
            FileKind::StyleNotes => "style_notes",
            FileKind::Timeline => "timeline",
            FileKind::Strings => "strings",
            FileKind::Note => "note",
            FileKind::Game => "game",
            FileKind::Png => "png",
            FileKind::Glb => "glb",
            FileKind::Gltf => "gltf",
            FileKind::Wav => "wav",
            FileKind::Ogg => "ogg",
        }
    }

    pub fn parse(s: &str) -> Option<FileKind> {
        FileKind::ALL.into_iter().find(|k| k.name() == s)
    }

    /// JSON text: patched by JSON path, diffed by path.
    pub fn is_json(self) -> bool {
        !matches!(self, FileKind::StyleRules | FileKind::Rust | FileKind::Cargo | FileKind::StyleNotes | FileKind::Note) && !self.is_binary()
    }

    /// Bytes, not text: the store holds a pointer into the content store ([`crate::blobs`]);
    /// written with project_upload, read with project_read's `bytes`.
    pub fn is_binary(self) -> bool {
        matches!(self, FileKind::Png | FileKind::Glb | FileKind::Gltf | FileKind::Wav | FileKind::Ogg)
    }

    /// A module crate's file: validated by building it (`logic_build`).
    pub fn is_module(self) -> bool {
        matches!(self, FileKind::Rust | FileKind::Cargo | FileKind::Json)
    }
}

/// One file in the store.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectFile {
    pub kind: FileKind,
    pub text: Arc<str>,
    /// [`text_hash`] of `text`.
    pub hash: u64,
    /// 1 when created or first imported, +1 per change (undo included).
    pub version: u64,
    pub modified_by: Arc<str>,
}

impl ProjectFile {
    pub fn new(kind: FileKind, text: Arc<str>, version: u64, by: Arc<str>) -> ProjectFile {
        ProjectFile { kind, hash: text_hash(&text), text, version, modified_by: by }
    }
}

/// The content hash files are compared by (store against disk, versions in conflict help).
pub fn text_hash(text: &str) -> u64 {
    let mut h = StableHasher::with_seed(0x7072_6f6a_6563_7400);
    h.write_str(text);
    h.finish()
}

/// Whether a journal path is taste state (`/project/assets/taste/...`): galleries, picks,
/// suggestions, boards and onboarding, written only through the taste tools and the window
/// (`Rule::Taste`, docs/design/GALLERIES.md). Approved results live elsewhere. Compared folded
/// (`assets/Taste/` is the same folder on Windows and macOS: taste review 2), and the folder
/// itself counts (a claim on it).
pub fn is_taste_state(journal: &str) -> bool {
    let f = fold_case(journal);
    f.starts_with("/PROJECT/ASSETS/TASTE/") || f == "/PROJECT/ASSETS/TASTE"
}

/// `/project/<path>`.
pub fn journal_path(path: &str) -> String {
    format!("/{ROOT}/{path}")
}

/// The project path of a journal path (`None` for anything outside `/project/`).
pub fn project_path_of(journal: &str) -> Option<&str> {
    journal.strip_prefix("/project/").filter(|p| !p.is_empty())
}

/// The journal id of a file change (see [`FILE_ID_BASE`]).
pub fn file_id(path: &str) -> u64 {
    FILE_ID_BASE | (specula_core::hash::hash_str(path) >> 1)
}

/// `/project` or anything below it.
pub fn under_root(path: &str) -> bool {
    path == "/project" || path.starts_with("/project/")
}

/// Binary bytes the store's current files may name in all (each at most
/// [`crate::blobs::MAX_BLOB_BYTES`]).
pub const MAX_BINARY_STORE_BYTES: u64 = 1 << 30;

/// The bytes a binary file's pointer names (0 for text files).
pub fn binary_bytes(f: &ProjectFile) -> u64 {
    if f.kind.is_binary() {
        crate::blobs::parse_pointer(&f.text).map_or(0, |(_, b)| b)
    } else {
        0
    }
}

/// Refuses putting a binary file of `bytes` at `path` when the store's binary files would pass
/// [`MAX_BINARY_STORE_BYTES`].
pub fn room_for_binary(files: &BTreeMap<String, ProjectFile>, path: &str, bytes: u64) -> Result<(), WorldError> {
    if bytes > crate::blobs::MAX_BLOB_BYTES {
        return Err(bad("too_large", format!("{path} would be {bytes} bytes; binary project files are at most {}", crate::blobs::MAX_BLOB_BYTES), path).with("bytes", bytes).with("limit", crate::blobs::MAX_BLOB_BYTES));
    }
    let total: u64 = files.iter().filter(|(p, _)| p.as_str() != path).map(|(_, f)| binary_bytes(f)).sum::<u64>() + bytes;
    if total > MAX_BINARY_STORE_BYTES {
        return Err(bad("store_full", format!("the project's binary files would hold {total} bytes; at most {MAX_BINARY_STORE_BYTES}: delete some first"), path).with("limit", MAX_BINARY_STORE_BYTES));
    }
    Ok(())
}

/// Folders binary files of each kind are mirrored in (any folder on the way counts:
/// `assets/textures/stone/wall.png`). Binary files elsewhere are left alone, so a working folder
/// full of screenshots is never copied into the content store.
pub const PNG_FOLDERS: [&str; 4] = ["textures", "images", "references", "icons"];
pub const MODEL_FOLDERS: [&str; 1] = ["models"];
pub const WAV_FOLDERS: [&str; 3] = ["audio", "sounds", "music"];

fn binary_kind(folders: &[&str], ext: Option<&str>) -> Option<FileKind> {
    let within = |names: &[&str]| folders.iter().any(|f| names.contains(f));
    match ext? {
        "png" if within(&PNG_FOLDERS) => Some(FileKind::Png),
        "glb" if within(&MODEL_FOLDERS) => Some(FileKind::Glb),
        "gltf" if within(&MODEL_FOLDERS) => Some(FileKind::Gltf),
        "wav" if within(&WAV_FOLDERS) => Some(FileKind::Wav),
        "ogg" if within(&WAV_FOLDERS) => Some(FileKind::Ogg),
        _ => None,
    }
}

/// Refuses putting `bytes` of text at `path` when the file or the store would pass its limits
/// ([`MAX_FILE_BYTES`], [`MAX_FILES`], [`MAX_STORE_BYTES`]).
pub fn room_for(files: &BTreeMap<String, ProjectFile>, path: &str, bytes: usize) -> Result<(), WorldError> {
    if bytes > MAX_FILE_BYTES {
        return Err(bad("too_large", format!("{path} would be {bytes} bytes; project files are at most {MAX_FILE_BYTES}"), path).with("bytes", bytes).with("limit", MAX_FILE_BYTES));
    }
    let old = files.get(path).map(|f| f.text.len());
    // Taste state has a budget of its own (taste review 2: galleries filled the shared store and
    // broke everyone's project_write); each pool counts only its own files.
    let taste = is_taste_state(&journal_path(path));
    let (max_files, max_bytes, what) = if taste { (MAX_TASTE_FILES, MAX_TASTE_BYTES, "taste state (galleries: gallery_delete)") } else { (MAX_FILES, MAX_STORE_BYTES, "the project store") };
    let pool = || files.iter().filter(|(p, _)| is_taste_state(&journal_path(p)) == taste);
    if old.is_none() && pool().count() >= max_files {
        return Err(bad("store_full", format!("{what} holds {max_files} files, its limit; delete some first"), path).with("limit", max_files));
    }
    let total: usize = pool().map(|(_, f)| f.text.len()).sum::<usize>() - old.unwrap_or(0) + bytes;
    if total > max_bytes {
        return Err(bad("store_full", format!("{what} would hold {total} bytes of text; at most {max_bytes}"), path).with("limit", max_bytes));
    }
    Ok(())
}

fn bad(code: &'static str, message: impl Into<String>, path: &str) -> WorldError {
    WorldError::new(code, message).with("path", path)
}

/// Names Windows reserves (with any extension): never valid file names.
fn device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let numbered = |p: &str| stem.strip_prefix(p).is_some_and(|n| {
        let mut c = n.chars();
        matches!((c.next(), c.next()), (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None))
    });
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") || numbered("COM") || numbered("LPT")
}

/// `name` folded for case-insensitive comparison the way Windows compares file names (Unicode
/// simple uppercase): `Ä` and `ä` are one file there.
pub fn fold_case(name: &str) -> String {
    name.chars().flat_map(char::to_uppercase).collect()
}

/// Checks a project path (`assets/ui/hud.json`): `/`-separated names valid as entity names and
/// as file names on every platform (no `\ : * ? " < > |`, no leading `.`, no trailing `.` or
/// space, no device names), at most 16 segments.
pub fn check_path(path: &str) -> Result<(), WorldError> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(bad("invalid_path", "project paths are relative, like \"assets/ui/hud.json\" (project_list shows them)", path));
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() > 16 {
        return Err(bad("invalid_path", "project paths are at most 16 segments deep", path));
    }
    for s in &segments {
        validate_name(s).map_err(|e| bad("invalid_path", format!("{path}: {}", e.message), path))?;
        let why = if s.starts_with('.') {
            Some("names starting with '.' are hidden files, never mirrored")
        } else if s.ends_with('.') || s.ends_with(' ') {
            Some("names may not end with '.' or a space")
        } else if s.chars().any(|c| matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')) {
            Some("names may not contain \\ : * ? \" < > |")
        } else if device_name(s) {
            Some("that name is reserved by Windows")
        } else {
            None
        };
        if let Some(why) = why {
            return Err(bad("invalid_path", format!("{path}: {why}"), path));
        }
    }
    Ok(())
}

/// Which mount a file belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountKind {
    Assets,
    Module,
    /// A project's own folder ([`ROOT_MOUNT`]): its game file and its notes, and nothing else
    /// (assets and modules have mounts of their own).
    Root,
}

/// A folder mirrored under a name.
#[derive(Clone, Debug, PartialEq)]
pub struct Mount {
    /// `assets` or `modules/<name>`.
    pub name: String,
    pub root: PathBuf,
    pub kind: MountKind,
}

/// The kind of the file at `rel` (`/`-separated, inside a mount), or `None` when files there
/// aren't mirrored.
pub fn kind_for(mount: MountKind, rel: &str) -> Option<FileKind> {
    let segments: Vec<&str> = rel.split('/').collect();
    let name = *segments.last()?;
    let parent = segments.len().checked_sub(2).map(|i| segments[i]);
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match mount {
        MountKind::Assets => {
            if name == "style.json" {
                return Some(FileKind::Style);
            }
            if name == "style.toml" {
                return Some(FileKind::StyleRules);
            }
            // The taste folder at the assets root (galleries, picks, boards, onboarding: private),
            // and the style folder (what the human approved from them: tracked).
            if segments.len() >= 2 && segments[0] == "taste" {
                return (ext.as_deref() == Some("json")).then_some(FileKind::Taste);
            }
            if segments.len() == 2 && segments[0] == "style" {
                return match ext.as_deref() {
                    Some("json") => Some(FileKind::StyleData),
                    Some("md") => Some(FileKind::StyleNotes),
                    _ => None,
                };
            }
            if let Some(k) = binary_kind(&segments[..segments.len() - 1], ext.as_deref()) {
                return Some(k);
            }
            if ext.as_deref() != Some("json") {
                return None;
            }
            match parent? {
                "recipes" => Some(FileKind::Recipe),
                "ui" if name == "theme.json" => Some(FileKind::UiTheme),
                "ui" => Some(FileKind::UiScreen),
                "sounds" => Some(FileKind::Sound),
                "music" => Some(FileKind::Music),
                "anims" => Some(FileKind::AnimSet),
                "timelines" => Some(FileKind::Timeline),
                "strings" => Some(FileKind::Strings),
                _ => None,
            }
        }
        MountKind::Module => match (segments.len(), segments[0], ext.as_deref()) {
            (1, "Cargo.toml", _) => Some(FileKind::Cargo),
            (1, _, Some("json")) => Some(FileKind::Json),
            (n, "src", Some("rs")) if n >= 2 => Some(FileKind::Rust),
            _ => None,
        },
        // The game file, and notes named by their ids (`notes/gate-lives.md`), flat.
        MountKind::Root => match segments.as_slice() {
            [GAME_FILE] => Some(FileKind::Game),
            [NOTES, file] => file.strip_suffix(".md").is_some_and(specula_memoria::id_ok).then_some(FileKind::Note),
            _ => None,
        },
    }
}

impl Mount {
    /// Where `rel` lives on disk.
    pub fn disk_path(&self, rel: &str) -> PathBuf {
        let mut p = self.root.clone();
        for s in rel.split('/') {
            p.push(s);
        }
        p
    }

    /// The project path of `rel` (the root mount's files are named as they lie).
    pub fn path_of(&self, rel: &str) -> String {
        if self.name.is_empty() {
            rel.to_owned()
        } else {
            format!("{}/{rel}", self.name)
        }
    }

    /// The path inside this mount that the project path `path` names, if it is this mount's.
    /// The root mount holds only the paths it mirrors (its game file and its notes): any
    /// other path is some other mount's, or nobody's.
    pub fn rel_of<'p>(&self, path: &'p str) -> Option<&'p str> {
        if self.name.is_empty() {
            return kind_for(self.kind, path).map(|_| path);
        }
        path.strip_prefix(&self.name).and_then(|r| r.strip_prefix('/')).filter(|r| !r.is_empty())
    }
}

/// A mount's files found on disk.
#[derive(Debug, Default)]
pub struct Scanned {
    /// `(rel path, kind)`, in path order.
    pub files: Vec<(String, FileKind)>,
    /// `(rel path, why)`: files of a known kind that weren't mirrored (too big, not UTF-8, bad name).
    pub skipped: Vec<(String, String)>,
    /// The entry budget ran out: some folders weren't looked at.
    pub truncated: bool,
}

/// Lists the files of known kinds under `mount` (see the module docs for what is skipped).
pub fn scan(mount: &Mount) -> Scanned {
    let mut out = Scanned::default();
    let mut budget = SCAN_ENTRIES;
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(mount.root.clone(), String::new(), 0)];
    while let Some((dir, rel, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut names: Vec<(String, std::fs::FileType)> = Vec::new();
        for entry in entries.flatten() {
            if budget == 0 {
                out.truncated = true;
                break;
            }
            budget -= 1;
            let Ok(ft) = entry.file_type() else { continue };
            let Ok(name) = entry.file_name().into_string() else { continue };
            names.push((name, ft));
        }
        names.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, ft) in names.into_iter().rev() {
            if name.starts_with('.') || ft.is_symlink() {
                continue;
            }
            let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            if ft.is_dir() {
                // The root mount looks at the project folder itself and its notes, never at the
                // rest of the project (assets, modules, build folders).
                if mount.kind == MountKind::Root && !(depth == 0 && name == NOTES) {
                    continue;
                }
                if depth + 1 < SCAN_DEPTH && !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push((dir.join(&name), child_rel, depth + 1));
                }
            } else if ft.is_file() {
                let Some(kind) = kind_for(mount.kind, &child_rel) else { continue };
                if let Err(e) = check_path(&mount.path_of(&child_rel)) {
                    out.skipped.push((child_rel, e.message));
                    continue;
                }
                out.files.push((child_rel, kind));
            }
        }
    }
    out.files.sort();
    out
}

/// What the engine last saw of a file on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub len: u64,
    pub mtime: Option<SystemTime>,
    /// [`text_hash`] of the text.
    pub hash: u64,
}

impl Stamp {
    fn same_file(&self, meta: (u64, Option<SystemTime>)) -> bool {
        (self.len, self.mtime) == meta
    }
}

/// Size and modification time of a regular file (links and folders are `None`).
fn stat(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    let m = std::fs::symlink_metadata(path).ok()?;
    m.file_type().is_file().then(|| (m.len(), m.modified().ok()))
}

/// Reads a mirrored file: at most [`MAX_FILE_BYTES`], UTF-8 (a byte-order mark is kept: it is
/// the file's).
pub fn read_text(path: &Path) -> Result<(Arc<str>, Stamp), String> {
    let (len, mtime) = stat(path).ok_or("not a regular file")?;
    if len as usize > MAX_FILE_BYTES {
        return Err(format!("{len} bytes; at most {MAX_FILE_BYTES} are mirrored"));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!("{} bytes; at most {MAX_FILE_BYTES} are mirrored", bytes.len()));
    }
    let text = String::from_utf8(bytes).map_err(|e| format!("not UTF-8 text (byte {})", e.utf8_error().valid_up_to()))?;
    let hash = text_hash(&text);
    // The time read after the content: a write landing in between shows as a changed stamp later.
    let (len2, mtime2) = stat(path).unwrap_or((len, mtime));
    let stamp = if (len2, mtime2) == (text.len() as u64, mtime) { Stamp { len: len2, mtime, hash } } else { Stamp { len: text.len() as u64, mtime: None, hash } };
    Ok((Arc::from(text), stamp))
}

/// Reads a binary file: at most [`crate::blobs::MAX_BLOB_BYTES`]; its bytes go into the content
/// store (`store`; `None` only computes the pointer) and the pointer text comes back.
pub fn read_blob(path: &Path, store: Option<&crate::blobs::ContentStore>) -> Result<(Arc<str>, Stamp), String> {
    let (len, mtime) = stat(path).ok_or("not a regular file")?;
    let max = crate::blobs::MAX_BLOB_BYTES;
    if len > max {
        return Err(format!("{len} bytes; binary files are mirrored up to {max}"));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max {
        return Err(format!("{} bytes; binary files are mirrored up to {max}", bytes.len()));
    }
    let id = match store {
        Some(s) => s.put(&bytes).map_err(|e| format!("not stored in the content store: {e}"))?,
        None => crate::blobs::BlobId::of(&bytes),
    };
    let text = crate::blobs::pointer(id, bytes.len() as u64);
    let hash = text_hash(&text);
    let (len2, mtime2) = stat(path).unwrap_or((len, mtime));
    let stamp = if (len2, mtime2) == (bytes.len() as u64, mtime) { Stamp { len: len2, mtime, hash } } else { Stamp { len: bytes.len() as u64, mtime: None, hash } };
    Ok((Arc::from(text), stamp))
}

/// Reads a mirrored file of `kind`: text ([`read_text`]) or binary ([`read_blob`]).
pub fn read_entry(path: &Path, kind: FileKind, store: Option<&crate::blobs::ContentStore>) -> Result<(Arc<str>, Stamp), String> {
    if kind.is_binary() {
        read_blob(path, store)
    } else {
        let read = read_text(path)?;
        // A timeline is parsed where it is read (the watcher's or the scan's thread), so the
        // engine thread never parses one to play or list it (docs/design/TIMELINE.md).
        if kind == FileKind::Timeline {
            crate::timeline::warm(&read.0, read.1.hash);
        }
        Ok(read)
    }
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `text` to `path` atomically: a hidden temporary file in the same folder, flushed, then
/// renamed over the target (folders inside the mount root are created; links are refused).
/// Returns the stamp of what is there afterwards and what it holds (someone may have written in
/// the same moment: the caller compares hashes).
pub fn write_atomic(root: &Path, path: &Path, text: &str) -> Result<(Stamp, Option<Arc<str>>), String> {
    write_entry(root, path, FileKind::Json, text, None)
}

/// [`write_atomic`] for a file of `kind`: a binary file's bytes come from the content store
/// (its text is the pointer), and are checked against it when read back.
pub fn write_entry(root: &Path, path: &Path, kind: FileKind, text: &str, store: Option<&crate::blobs::ContentStore>) -> Result<(Stamp, Option<Arc<str>>), String> {
    use std::io::Write;
    let bytes: std::borrow::Cow<[u8]> = if kind.is_binary() {
        let (id, _) = crate::blobs::parse_pointer(text).ok_or("not a binary file's pointer")?;
        let store = store.ok_or("no content store")?;
        std::borrow::Cow::Owned(store.get(id).map_err(|e| format!("its content isn't in the content store ({e})"))?)
    } else {
        std::borrow::Cow::Borrowed(text.as_bytes())
    };
    let parent = path.parent().ok_or("no parent folder")?;
    check_inside(root, parent)?;
    // Through the disk budget: below the floor the write waits (pending, retried) instead.
    let old = crate::disk::file_len(path).unwrap_or(0);
    crate::disk::book().admit(crate::disk::Category::Project, (bytes.len() as u64).saturating_sub(old), crate::disk::Need::Agent).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("can't create {}: {e}", parent.display()))?;
    check_inside(root, parent)?;
    let name = path.file_name().and_then(|n| n.to_str()).ok_or("bad file name")?;
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!(".{name}.specula-tmp-{}-{n}", std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    // Read back and compared byte for byte (never only by hash: hashes can be made to collide).
    // Only someone else's bytes (a race) go into the content store.
    let Some((len, mtime)) = stat(path) else { return Err("written, but can't be read back: not a regular file".into()) };
    let now = std::fs::read(path).map_err(|e| format!("written, but can't be read back: {e}"))?;
    if now == *bytes {
        let stamp = if stat(path) == Some((len, mtime)) { Stamp { len, mtime, hash: text_hash(text) } } else { Stamp { len, mtime: None, hash: text_hash(text) } };
        return Ok((stamp, None));
    }
    match read_entry(path, kind, store) {
        Ok((theirs, stamp)) => Ok((stamp, Some(theirs))),
        Err(e) => Err(format!("written, but can't be read back: {e}")),
    }
}

/// Refuses folders that leave `root` through a link (each existing folder from `root` down to
/// `dir` must be a real folder).
pub fn check_inside(root: &Path, dir: &Path) -> Result<(), String> {
    let rel = dir.strip_prefix(root).map_err(|_| format!("{} is outside the mount", dir.display()))?;
    let mut at = root.to_path_buf();
    for c in rel.components() {
        at.push(c);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if m.file_type().is_symlink() => return Err(format!("{} is a link; the engine doesn't write through links", at.display())),
            Ok(m) if !m.is_dir() => return Err(format!("{} is a file, not a folder", at.display())),
            Ok(_) => {}
            Err(_) => break, // the rest doesn't exist yet
        }
    }
    Ok(())
}

/// What the watcher found on disk.
#[derive(Clone, Debug)]
pub enum Event {
    /// A mirrored file (or a new file of a known kind) whose text differs from what was synced.
    Changed { path: String, kind: FileKind, text: Arc<str>, stamp: Stamp },
    /// A mirrored file that is gone.
    Removed { path: String },
    /// A mirrored file changed on disk into something that can't be mirrored (not UTF-8, or
    /// larger than [`MAX_FILE_BYTES`]): it is held (never overwritten) until it is fixed on disk or
    /// an agent replaces it on purpose (its bytes are backed up first).
    Unreadable { path: String, len: u64, mtime: Option<SystemTime>, why: String },
}

impl Event {
    pub fn path(&self) -> &str {
        match self {
            Event::Changed { path, .. } | Event::Removed { path } | Event::Unreadable { path, .. } => path,
        }
    }
}

/// What the watcher and the main thread share.
#[derive(Default)]
struct Shared {
    mounts: Mutex<Vec<Mount>>,
    /// Project path → what the engine last synced (wrote, read or imported).
    stamps: Mutex<BTreeMap<String, Stamp>>,
    stop: AtomicBool,
    /// Where binary files' bytes live (see [`crate::blobs`]).
    content: std::sync::OnceLock<Arc<crate::blobs::ContentStore>>,
}

/// A disk change the engine found while writing (someone wrote the same file at the same time),
/// or a write that failed: what `project_list` reports.
#[derive(Clone, Debug, PartialEq)]
pub struct DiskNote {
    pub path: String,
    pub message: String,
}

/// One path a sync looks at: where it is on disk, and what the store has (`None`: deleted).
pub struct SyncItem {
    path: String,
    at: Option<(Mount, String)>,
    file: Option<(FileKind, Arc<str>, u64)>,
}

/// A sync planned on the engine thread ([`Disk::sync_plan`]), run anywhere ([`SyncPlan::execute`]:
/// the disk work, with the temporary file's `fsync`), applied on the engine thread
/// ([`Disk::sync_apply`]). Stamps are written as each file is written (the watcher must never see
/// the engine's own write as an outside edit), through the shared state.
pub struct SyncPlan {
    items: Vec<SyncItem>,
    revision: u64,
    shared: Arc<Shared>,
}

/// What a sync found and did at one path.
#[derive(Debug)]
pub enum Outcome {
    /// The path isn't mounted any more (forgotten).
    NotMounted,
    /// Touched on disk with the same text (its new time is stamped).
    Touched,
    /// The disk was put back to the store's text: nothing to write, nothing held.
    Restored,
    /// Someone changed the file since it was last synced: their text, to import.
    Raced(Arc<str>, Stamp),
    /// The store deleted it and the disk no longer has it.
    Gone,
    /// It changed on disk into something that can't be mirrored: held.
    Unreadable { len: u64, mtime: Option<SystemTime>, why: String },
    /// Written (or deleted) as the store has it.
    Written,
    /// Written, but someone wrote it in the same moment: theirs, to import.
    WriteRaced(Arc<str>, Stamp),
    /// The write or delete failed (retried later).
    Failed(String),
}

/// What [`SyncPlan::execute`] brings back.
pub struct SyncDone {
    revision: u64,
    outcomes: Vec<(String, Outcome)>,
    /// Paths found readable again (no longer held).
    readable: Vec<String>,
}

impl SyncDone {
    /// The paths it wrote or deleted.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.outcomes.iter().map(|(p, _)| p.as_str())
    }
}

impl SyncPlan {
    /// Paths it will look at.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The disk work: for each path, what changed on disk since it was last synced (theirs
    /// comes in, nothing is lost), else the write or delete.
    pub fn execute(self) -> SyncDone {
        let shared = &self.shared;
        let content = shared.content.get().map(|c| &**c);
        let stamp = |path: &str, s: Stamp| {
            shared.stamps.lock().unwrap_or_else(|p| p.into_inner()).insert(path.to_owned(), s);
        };
        let unstamp = |path: &str| {
            shared.stamps.lock().unwrap_or_else(|p| p.into_inner()).remove(path);
        };
        let mut outcomes = Vec::with_capacity(self.items.len());
        let mut readable = Vec::new();
        for item in self.items {
            let SyncItem { path, at, file } = item;
            let Some((mount, rel)) = at else {
                unstamp(&path);
                outcomes.push((path, Outcome::NotMounted));
                continue;
            };
            let disk = mount.disk_path(&rel);
            // What changed on disk since it was last synced (Disk::refresh's rule).
            let known = shared.stamps.lock().unwrap_or_else(|p| p.into_inner()).get(&path).copied();
            let event = match (kind_for(mount.kind, &rel), stat(&disk), known) {
                (None, _, _) | (_, None, None) => None,
                (Some(_), None, Some(_)) => Some(Event::Removed { path: path.clone() }),
                (Some(_), Some(meta), Some(s)) if s.same_file(meta) => None,
                (Some(kind), Some(meta), _) => match read_entry(&disk, kind, content) {
                    Ok((text, st)) if Some(st.hash) != known.map(|s| s.hash) => Some(Event::Changed { path: path.clone(), kind, text, stamp: st }),
                    Ok((_, st)) => {
                        // Touched, same text: remember the new time; readable again, so not held.
                        readable.push(path.clone());
                        stamp(&path, st);
                        None
                    }
                    Err(why) => Some(Event::Unreadable { path: path.clone(), len: meta.0, mtime: meta.1, why }),
                },
            };
            if let Some(event) = event {
                let o = match event {
                    Event::Changed { text, stamp: st, .. } => {
                        if file.as_ref().is_some_and(|(_, _, hash)| *hash == st.hash) {
                            stamp(&path, st);
                            Outcome::Restored
                        } else {
                            Outcome::Raced(text, st)
                        }
                    }
                    Event::Removed { .. } if file.is_none() => {
                        unstamp(&path);
                        Outcome::Gone
                    }
                    // Write it back (the store has it).
                    Event::Removed { .. } => Outcome::Touched,
                    Event::Unreadable { len, mtime, why, .. } => Outcome::Unreadable { len, mtime, why },
                };
                if !matches!(o, Outcome::Touched) {
                    outcomes.push((path, o));
                    continue;
                }
            }
            // A module folder being built is left alone until its build has loaded (the write
            // is retried): the sources a build digests are the sources it compiles.
            if specula_logic::build::building(&disk) {
                outcomes.push((path, Outcome::Failed(BUILDING.into())));
                continue;
            }
            let o = match &file {
                Some((kind, text, _)) => match write_entry(&mount.root, &disk, *kind, text, content) {
                    Ok((st, None)) => {
                        stamp(&path, st);
                        Outcome::Written
                    }
                    Ok((st, Some(theirs))) => Outcome::WriteRaced(theirs, st),
                    Err(e) => Outcome::Failed(e),
                },
                None => match disk.parent().map_or(Ok(()), |d| check_inside(&mount.root, d)).map_err(std::io::Error::other).and_then(|()| std::fs::remove_file(&disk)) {
                    Ok(()) => {
                        unstamp(&path);
                        Outcome::Written
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        unstamp(&path);
                        Outcome::Written
                    }
                    Err(e) => Outcome::Failed(e.to_string()),
                },
            };
            outcomes.push((path, o));
        }
        SyncDone { revision: self.revision, outcomes, readable }
    }
}

/// What a sync did.
#[derive(Debug, Default)]
pub struct Synced {
    /// Files written or deleted (project paths): what changed for hot reload.
    pub changed: Vec<String>,
    /// Files whose write failed, with the error (retried later).
    pub failed: Vec<(String, String)>,
    /// Files someone else wrote meanwhile: `(path, their text, stamp)`, to import.
    pub raced: Vec<(String, Arc<str>, Stamp)>,
}

/// The engine's side of the disk: mounts, what was last synced, the watcher thread and the
/// writes that failed.
#[derive(Default)]
pub struct Disk {
    shared: Arc<Shared>,
    events: Option<Receiver<Event>>,
    watcher: Option<std::thread::JoinHandle<()>>,
    /// The world's files revision last synced.
    pub synced_revision: u64,
    /// Project path → the error of its last failed write (retried after every commit and once a
    /// second).
    pub pending: BTreeMap<String, String>,
    /// Recent disk conflicts (newest last).
    pub conflicts: VecDeque<DiskNote>,
    /// Per mount: files of a known kind not mirrored, and whether its scan was cut short.
    pub skipped: BTreeMap<String, (Vec<(String, String)>, bool)>,
    /// Files changed on disk into something that can't be mirrored: path -> (size, time, why).
    /// Never overwritten while held (see [`Event::Unreadable`]).
    pub held: BTreeMap<String, (u64, Option<SystemTime>, String)>,
}

impl Drop for Disk {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.watcher.take() {
            let _ = h.join();
        }
    }
}

impl Disk {
    /// The content store binary files read into and write from (set once, before mounting).
    pub fn set_content(&self, store: Arc<crate::blobs::ContentStore>) {
        let _ = self.shared.content.set(store);
    }

    pub fn content(&self) -> Option<&Arc<crate::blobs::ContentStore>> {
        self.shared.content.get()
    }

    pub fn mounts(&self) -> Vec<Mount> {
        self.shared.mounts.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// The mount holding `path`, and the path inside it.
    pub fn locate(&self, path: &str) -> Option<(Mount, String)> {
        let mounts = self.shared.mounts.lock().unwrap_or_else(|p| p.into_inner());
        mounts.iter().find_map(|m| m.rel_of(path).map(|r| (m.clone(), r.to_owned())))
    }

    /// Adds a mount and reads its files: `(project path, kind, text, stamp)` for the caller to
    /// import (and then [`Disk::stamp`]). Starts the watcher with the first mount.
    pub fn mount(&mut self, mount: Mount) -> Vec<(String, FileKind, Arc<str>, Stamp)> {
        let found = scan(&mount);
        let mut skipped = found.skipped;
        let mut files = Vec::new();
        for (rel, kind) in found.files {
            match read_entry(&mount.disk_path(&rel), kind, self.content().map(|c| &**c)) {
                Ok((text, stamp)) => files.push((mount.path_of(&rel), kind, text, stamp)),
                Err(e) => skipped.push((rel, e)),
            }
        }
        self.skipped.insert(mount.name.clone(), (skipped, found.truncated));
        {
            let mut mounts = self.shared.mounts.lock().unwrap_or_else(|p| p.into_inner());
            mounts.retain(|m| m.name != mount.name);
            mounts.push(mount);
        }
        self.start_watcher();
        files
    }

    pub fn module_mounts(&self) -> usize {
        self.shared.mounts.lock().unwrap_or_else(|p| p.into_inner()).iter().filter(|m| m.kind == MountKind::Module).count()
    }

    fn start_watcher(&mut self) {
        if self.watcher.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let shared = Arc::clone(&self.shared);
        match std::thread::Builder::new().name("specula-project-watch".into()).spawn(move || watch(&shared, &tx)) {
            Ok(h) => {
                self.watcher = Some(h);
                self.events = Some(rx);
            }
            Err(e) => eprintln!("specula: project files won't notice edits made outside the engine: {e}"),
        }
    }

    /// Records what the disk holds for `path` now (after a write, a read or an import).
    pub fn stamp(&self, path: &str, stamp: Stamp) {
        self.shared.stamps.lock().unwrap_or_else(|p| p.into_inner()).insert(path.to_owned(), stamp);
    }

    pub fn stamp_of(&self, path: &str) -> Option<Stamp> {
        self.shared.stamps.lock().unwrap_or_else(|p| p.into_inner()).get(path).copied()
    }

    /// Forgets `path` (it left the store and the disk).
    pub fn forget(&self, path: &str) {
        self.unstamp(path);
    }

    fn unstamp(&self, path: &str) {
        self.shared.stamps.lock().unwrap_or_else(|p| p.into_inner()).remove(path);
    }

    /// What the watcher found since the last call (never blocks).
    pub fn drain(&self) -> Vec<Event> {
        self.events.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default()
    }

    /// What changed on disk at `path` since it was last synced, as the event to import (`None`:
    /// nothing, or `path` isn't mounted).
    pub fn refresh(&mut self, path: &str) -> Option<Event> {
        let (mount, rel) = self.locate(path)?;
        let kind = kind_for(mount.kind, &rel)?;
        let disk = mount.disk_path(&rel);
        let known = self.stamp_of(path);
        match (stat(&disk), known) {
            (None, Some(_)) => Some(Event::Removed { path: path.to_owned() }),
            (None, None) => None,
            (Some(meta), Some(s)) if s.same_file(meta) => None,
            (Some(meta), _) => match read_entry(&disk, kind, self.content().map(|c| &**c)) {
                Ok((text, stamp)) if Some(stamp.hash) != known.map(|s| s.hash) => Some(Event::Changed { path: path.to_owned(), kind, text, stamp }),
                Ok((_, stamp)) => {
                    // Touched, same text: remember the new time; readable again, so not held.
                    self.held.remove(path);
                    self.stamp(path, stamp);
                    None
                }
                Err(why) => Some(Event::Unreadable { path: path.to_owned(), len: meta.0, mtime: meta.1, why }),
            },
        }
    }

    /// Whether `event` still describes the disk (a newer write may have overtaken it).
    pub fn still_current(&self, event: &Event) -> bool {
        let Some((mount, rel)) = self.locate(event.path()) else {
            return false;
        };
        let now = stat(&mount.disk_path(&rel));
        match event {
            Event::Changed { stamp, .. } => now.is_some_and(|m| stamp.same_file(m) || stamp.mtime.is_none()),
            Event::Removed { .. } => now.is_none(),
            Event::Unreadable { len, mtime, .. } => now == Some((*len, *mtime)),
        }
    }

    /// Makes the disk hold what `files` holds for every mounted path: writes files whose text
    /// isn't what was last synced (atomically), deletes files the store no longer has. A file
    /// changed on disk since it was last synced is never overwritten or deleted: it comes back
    /// in `raced`, to import (the human's edit wins; the store's version stays in the journal).
    ///
    /// All at once, on this thread: [`Disk::sync_plan`], [`SyncPlan::execute`] (the disk work,
    /// which the engine runs on a worker) and [`Disk::sync_apply`].
    pub fn sync(&mut self, files: &BTreeMap<String, ProjectFile>, revision: u64) -> Synced {
        let plan = self.sync_plan(files, revision);
        let done = plan.execute();
        self.sync_apply(done)
    }

    /// What a sync must look at (no disk access: stamps and mounts are in memory): files whose
    /// text isn't what was last synced, and files the store no longer has.
    pub fn sync_plan(&self, files: &BTreeMap<String, ProjectFile>, revision: u64) -> SyncPlan {
        let stamps = self.shared.stamps.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let mut items: Vec<SyncItem> = Vec::new();
        for (path, f) in files {
            if stamps.get(path).map(|s| s.hash) != Some(f.hash) {
                items.push(SyncItem { path: path.clone(), at: self.locate(path), file: Some((f.kind, Arc::clone(&f.text), f.hash)) });
            }
        }
        for path in stamps.keys() {
            if !files.contains_key(path) {
                items.push(SyncItem { path: path.clone(), at: self.locate(path), file: None });
            }
        }
        SyncPlan { items, revision, shared: Arc::clone(&self.shared) }
    }

    /// Applies what a sync found and did (see [`Outcome`]): pending writes, held files and what
    /// changed, raced or failed.
    pub fn sync_apply(&mut self, done: SyncDone) -> Synced {
        let mut out = Synced::default();
        for (path, o) in done.outcomes {
            match o {
                Outcome::NotMounted | Outcome::Touched => {}
                Outcome::Restored => {
                    self.held.remove(&path);
                    self.pending.remove(&path);
                }
                Outcome::Raced(text, stamp) => out.raced.push((path, text, stamp)),
                Outcome::Gone => {
                    self.pending.remove(&path);
                    out.changed.push(path);
                }
                Outcome::Unreadable { len, mtime, why } => {
                    let e = format!("disk_unreadable: the file on disk changed and can't be mirrored ({why}); it was not overwritten. Fix it on disk, or replace it with replace_unreadable: true (its bytes are backed up first)");
                    self.held.insert(path.clone(), (len, mtime, why));
                    self.pending.insert(path.clone(), e.clone());
                    out.failed.push((path, e));
                }
                Outcome::Written => {
                    self.pending.remove(&path);
                    out.changed.push(path);
                }
                Outcome::WriteRaced(theirs, stamp) => {
                    self.pending.remove(&path);
                    out.raced.push((path, theirs, stamp));
                }
                Outcome::Failed(e) => {
                    self.pending.insert(path.clone(), e.clone());
                    out.failed.push((path, e));
                }
            }
        }
        // Touched files are readable again (refresh's own rule).
        for p in done.readable {
            self.held.remove(&p);
        }
        if out.failed.is_empty() && out.raced.is_empty() {
            self.synced_revision = done.revision;
        }
        out
    }

    /// Copies the bytes on disk at `path` (a held, unreadable file) next to it as a hidden backup
    /// (never mirrored), and lets the next write replace the file. Returns the backup's path.
    pub fn back_up_and_release(&mut self, path: &str) -> Result<PathBuf, String> {
        let (mount, rel) = self.locate(path).ok_or("not mounted")?;
        let disk = mount.disk_path(&rel);
        let parent = disk.parent().ok_or("no folder")?;
        check_inside(&mount.root, parent)?;
        let name = disk.file_name().and_then(|n| n.to_str()).ok_or("bad name")?;
        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let backup = parent.join(format!(".{name}.specula-backup-{}-{n}", std::process::id()));
        crate::disk::book().copy(crate::disk::Category::Project, "backup", &disk, &backup, crate::disk::Need::Agent).map_err(|e| format!("can't back up {}: {e}", disk.display()))?;
        let meta = stat(&disk).ok_or("gone")?;
        // What is there now counts as synced (hash 0 never matches a text), so the write proceeds.
        self.stamp(path, Stamp { len: meta.0, mtime: meta.1, hash: 0 });
        self.held.remove(path);
        self.pending.remove(path);
        Ok(backup)
    }

    /// Remembers a disk conflict for `project_list`.
    pub fn note_conflict(&mut self, path: &str, message: String) {
        if self.conflicts.len() == MAX_CONFLICTS {
            self.conflicts.pop_front();
        }
        self.conflicts.push_back(DiskNote { path: path.to_owned(), message });
    }
}

/// The watcher thread: stats every mirrored file once a second, looks for new files every few
/// seconds, and reports text that differs from what was synced (each change once).
fn watch(shared: &Shared, tx: &Sender<Event>) {
    let mut sent: BTreeMap<String, (u64, Option<SystemTime>)> = BTreeMap::new();
    let mut round = 0u32;
    while !shared.stop.load(Ordering::Relaxed) {
        std::thread::sleep(WATCH_EVERY);
        if shared.stop.load(Ordering::Relaxed) {
            return;
        }
        let mounts = shared.mounts.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let stamps = shared.stamps.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut check = |path: String, kind: FileKind, disk: PathBuf, stamp: Option<Stamp>| -> bool {
            let meta = stat(&disk);
            match (meta, stamp) {
                (None, Some(_)) => {
                    if sent.get(&path) != Some(&(u64::MAX, None)) {
                        sent.insert(path.clone(), (u64::MAX, None));
                        return tx.send(Event::Removed { path }).is_ok();
                    }
                }
                (Some(m), s) if s.is_none_or(|s| !s.same_file(m)) && sent.get(&path) != Some(&m) => {
                    sent.insert(path.clone(), m);
                    match read_entry(&disk, kind, shared.content.get().map(|c| &**c)) {
                        // Same text as synced still goes in: the engine stamps the new time, and a
                        // file held as unreadable is released once the disk is readable again.
                        Ok((text, st)) => return tx.send(Event::Changed { path, kind, text, stamp: st }).is_ok(),
                        // Mirrored files that turn unreadable are held; new ones are just skipped.
                        Err(why) if s.is_some() => return tx.send(Event::Unreadable { path, len: m.0, mtime: m.1, why }).is_ok(),
                        Err(_) => {}
                    }
                }
                _ => {}
            }
            true
        };
        for m in &mounts {
            // (The root mount's few paths have no prefix of their own: every stamp is asked.)
            let from = if m.name.is_empty() { String::new() } else { format!("{}/", m.name) };
            for (path, stamp) in stamps.range(from..) {
                let Some(rel) = m.rel_of(path) else {
                    if m.name.is_empty() {
                        continue;
                    }
                    break;
                };
                let Some(kind) = kind_for(m.kind, rel) else { continue };
                seen.insert(path.clone());
                if !check(path.clone(), kind, m.disk_path(rel), Some(*stamp)) {
                    return;
                }
            }
        }
        round += 1;
        if round.is_multiple_of(RESCAN_EVERY) {
            for m in &mounts {
                for (rel, kind) in scan(m).files {
                    let path = m.path_of(&rel);
                    if !seen.contains(&path) && !stamps.contains_key(&path) && !check(path, kind, m.disk_path(&rel), None) {
                        return;
                    }
                }
            }
        }
        // Forget what was reported once the main thread caught up.
        sent.retain(|p, m| stamps.get(p).is_none_or(|s| !s.same_file(*m)) && (m.0 != u64::MAX || stamps.contains_key(p)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_place() {
        let a = MountKind::Assets;
        assert_eq!(kind_for(a, "recipes/tombstone.json"), Some(FileKind::Recipe));
        assert_eq!(kind_for(a, "games/x/assets/recipes/t.json"), Some(FileKind::Recipe));
        assert_eq!(kind_for(a, "ui/hud.json"), Some(FileKind::UiScreen));
        assert_eq!(kind_for(a, "ui/theme.json"), Some(FileKind::UiTheme));
        assert_eq!(kind_for(a, "sounds/ui_click.json"), Some(FileKind::Sound));
        assert_eq!(kind_for(a, "anims/wraith.json"), Some(FileKind::AnimSet));
        assert_eq!(kind_for(a, "timelines/intro.json"), Some(FileKind::Timeline));
        assert_eq!(kind_for(a, "strings/de.json"), Some(FileKind::Strings));
        assert_eq!(kind_for(a, "timelines/notes.txt"), None);
        assert_eq!(kind_for(a, "style.json"), Some(FileKind::Style));
        assert_eq!(kind_for(a, "x/style.toml"), Some(FileKind::StyleRules));
        assert_eq!(kind_for(a, "recipes/readme.md"), None);
        assert_eq!(kind_for(a, "textures/stone/wall.png"), Some(FileKind::Png));
        assert_eq!(kind_for(a, "models/wraith.glb"), Some(FileKind::Glb));
        assert_eq!(kind_for(a, "models/wraith.gltf"), Some(FileKind::Gltf));
        assert_eq!(kind_for(a, "sounds/bell.wav"), Some(FileKind::Wav));
        assert_eq!(kind_for(a, "music/map1.json"), Some(FileKind::Music));
        assert_eq!(kind_for(a, "music/theme.ogg"), Some(FileKind::Ogg));
        assert_eq!(kind_for(a, "audio/rain.ogg"), Some(FileKind::Ogg));
        assert_eq!(kind_for(a, "docs/clip.ogg"), None, "audio files only in their folders");
        assert_eq!(kind_for(a, "docs/gallery/shot.png"), None, "binary files only in their folders");
        assert_eq!(kind_for(a, "luts/grade.png"), None);
        assert_eq!(kind_for(a, "data/waves.json"), None);
        assert_eq!(kind_for(a, "taste/galleries/moon.json"), Some(FileKind::Taste));
        assert_eq!(kind_for(a, "taste/style_notes.md"), None, "notes are approved results: style/");
        assert_eq!(kind_for(a, "style/notes.md"), Some(FileKind::StyleNotes));
        assert_eq!(kind_for(a, "style/priors.json"), Some(FileKind::StyleData));
        assert_eq!(kind_for(a, "taste/boards/moonlit/a.png"), None, "board images are on disk, not in the store");
        assert_eq!(kind_for(a, "recipes/taste/x.json"), None);
        let m = MountKind::Module;
        assert_eq!(kind_for(m, "Cargo.toml"), Some(FileKind::Cargo));
        assert_eq!(kind_for(m, "actions.json"), Some(FileKind::Json));
        assert_eq!(kind_for(m, "src/lib.rs"), Some(FileKind::Rust));
        assert_eq!(kind_for(m, "src/a/b.rs"), Some(FileKind::Rust));
        assert_eq!(kind_for(m, "lib.rs"), None);
        assert_eq!(kind_for(m, "target/x.json"), None);
        let r = MountKind::Root;
        assert_eq!(kind_for(r, "game.json"), Some(FileKind::Game));
        assert_eq!(kind_for(r, "notes/gate-lives.md"), Some(FileKind::Note));
        assert_eq!(kind_for(r, "notes/Gate Lives.md"), None, "a note's file is named by its id");
        assert_eq!(kind_for(r, "notes/old/gate-lives.md"), None, "notes are flat");
        assert_eq!(kind_for(r, "notes/gate-lives.txt"), None);
        assert_eq!(kind_for(r, "gate-lives.md"), None);
        assert_eq!(kind_for(r, "checks/game.json"), None, "the game file is at the root");
        assert_eq!(kind_for(r, "assets/ui/hud.json"), None, "assets have their own mount");
        assert_eq!(kind_for(a, "notes/gate-lives.md"), None, "a notes folder under assets is not the project's memory");
        assert_eq!(kind_for(a, "game.json"), None);
        // The root mount's files are named as they lie, and it holds no other path.
        let root = Mount { name: ROOT_MOUNT.into(), root: PathBuf::from("p"), kind: r };
        let assets = Mount { name: ASSETS.into(), root: PathBuf::from("p/assets"), kind: a };
        assert_eq!((root.path_of("game.json"), root.rel_of("game.json"), root.rel_of("notes/a.md")), ("game.json".to_owned(), Some("game.json"), Some("notes/a.md")));
        assert_eq!((root.rel_of("assets/ui/hud.json"), root.rel_of("notes"), root.rel_of("nowhere/x.json")), (None, None, None));
        assert_eq!((assets.path_of("ui/hud.json"), assets.rel_of("assets/ui/hud.json"), assets.rel_of("assets"), assets.rel_of("game.json")), ("assets/ui/hud.json".to_owned(), Some("ui/hud.json"), None, None));
    }

    /// Taste review 2: galleries shared the project store's budget, so any agent could fill it
    /// and break everyone's project_write. Each pool counts only its own files.
    #[test]
    fn taste_state_has_a_budget_of_its_own() {
        let mut files = BTreeMap::new();
        for i in 0..MAX_TASTE_FILES {
            files.insert(format!("assets/taste/galleries/g{i}.json"), ProjectFile::new(FileKind::Taste, Arc::from("{}"), 1, Arc::from("t")));
        }
        assert_eq!(room_for(&files, "assets/taste/galleries/one_more.json", 2).unwrap_err().code, "store_full", "taste state is full");
        assert!(room_for(&files, "assets/ui/hud.json", 2).is_ok(), "the project's own files still have room");
        assert!(room_for(&files, "assets/taste/galleries/g0.json", 4).is_ok(), "rewriting one is fine");
    }

    #[test]
    fn device_names_and_case_folding() {
        for bad in ["assets/recipes/COM\u{b9}.json", "assets/recipes/conin$.json", "assets/recipes/CONOUT$.json", "assets/recipes/lpt0.json", "assets/aux"] {
            assert!(check_path(bad).is_err(), "{bad:?} names a device");
        }
        assert!(check_path("assets/recipes/COM10.json").is_ok() && check_path("assets/recipes/console.json").is_ok());
        assert_eq!(fold_case("assets/\u{c4}/t.json"), fold_case("assets/\u{e4}/t.json"));
        assert_ne!(fold_case("a"), fold_case("b"));
    }

    #[test]
    fn paths_are_checked() {
        assert!(check_path("assets/ui/hud.json").is_ok());
        for bad in ["", "/assets/x.json", "assets/", "assets//x.json", "assets/.x.json", "assets/x.json.", "assets/a:b.json", "assets/con.json", "assets/../x.json", "assets\\x.json"] {
            assert!(check_path(bad).is_err(), "{bad:?} should be refused");
        }
        assert_eq!(project_path_of("/project/assets/x.json"), Some("assets/x.json"));
        assert_eq!(project_path_of("/projects/x"), None);
        assert!(file_id("a") >= FILE_ID_BASE && file_id("a") != file_id("b"));
        assert!(file_id("a") > crate::world::CAMERA_ID_BASE | (u64::MAX >> 2));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-project-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scans_known_kinds_and_skips_the_rest() {
        let root = scratch("scan");
        for (p, t) in [("recipes/a.json", "{}"), ("recipes/.a.json.specula-tmp-1-0", "{}"), ("ui/hud.json", "{}"), ("ui/theme.json", "{}"), ("notes.txt", "x"), ("target/recipes/b.json", "{}"), ("style.json", "{}")] {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        }
        std::fs::write(root.join("recipes/big.json"), "x".repeat(MAX_FILE_BYTES + 1)).unwrap();
        let mut disk = Disk::default();
        let files = disk.mount(Mount { name: ASSETS.into(), root: root.clone(), kind: MountKind::Assets });
        let paths: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(paths, ["assets/recipes/a.json", "assets/style.json", "assets/ui/hud.json", "assets/ui/theme.json"]);
        assert_eq!(disk.skipped[ASSETS].0.len(), 1, "the big recipe is listed as skipped");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R2 of m5-gmods: an agent swapped a module's sources through project_write while
    /// the lead's approved build ran, and put them back before the build's second digest. A
    /// folder held for a build is left alone: the write waits and lands when the build ends.
    #[test]
    fn writes_into_a_module_folder_wait_while_it_is_built() {
        let root = scratch("building");
        std::fs::create_dir_all(root.join("sounds")).unwrap();
        std::fs::write(root.join("sounds/s.json"), "{\"a\": 1}").unwrap();
        std::fs::write(root.join("sounds/gone.json"), "{\"g\": 1}").unwrap();
        let mut disk = Disk::default();
        let mut files = BTreeMap::new();
        for (p, k, t, s) in disk.mount(Mount { name: ASSETS.into(), root: root.clone(), kind: MountKind::Assets }) {
            disk.stamp(&p, s);
            files.insert(p, ProjectFile::new(k, t, 1, Arc::from("disk")));
        }
        let path = "assets/sounds/s.json";
        let held = specula_logic::build::hold(&root);
        files.insert(path.into(), ProjectFile::new(FileKind::Sound, Arc::from("{\"evil\": 2}"), 2, Arc::from("mallory")));
        files.remove("assets/sounds/gone.json");
        let done = disk.sync(&files, 2);
        assert_eq!(done.failed.len(), 2, "the write and the delete both wait: {done:?}");
        assert!(done.failed.iter().all(|(_, e)| e.starts_with("module_building")), "{done:?}");
        assert!(done.changed.is_empty());
        assert_eq!(std::fs::read_to_string(root.join("sounds/s.json")).unwrap(), "{\"a\": 1}", "the disk is as the build read it");
        assert!(root.join("sounds/gone.json").exists());
        drop(held);
        let done = disk.sync(&files, 3);
        assert!(done.failed.is_empty(), "{done:?}");
        assert_eq!(std::fs::read_to_string(root.join("sounds/s.json")).unwrap(), "{\"evil\": 2}", "written once the build ended");
        assert!(!root.join("sounds/gone.json").exists());
    }

    /// Review finding: an outside edit that isn't UTF-8 (or is over 1 MiB) was never imported and the
    /// next write overwrote it. It is held now: reported, never overwritten, backed up on purpose.
    #[test]
    fn unreadable_outside_edits_are_held_not_overwritten() {
        let root = scratch("held");
        std::fs::create_dir_all(root.join("sounds")).unwrap();
        std::fs::write(root.join("sounds/s.json"), "{\"a\": 1}").unwrap();
        let mut disk = Disk::default();
        let mut files = BTreeMap::new();
        for (p, k, t, s) in disk.mount(Mount { name: ASSETS.into(), root: root.clone(), kind: MountKind::Assets }) {
            disk.stamp(&p, s);
            files.insert(p, ProjectFile::new(k, t, 1, Arc::from("disk")));
        }
        let path = "assets/sounds/s.json";
        for (i, human) in [b"{\"caf\xe9\": 1}".to_vec(), vec![b' '; MAX_FILE_BYTES + 1]].into_iter().enumerate() {
            std::thread::sleep(Duration::from_millis(20));
            std::fs::write(root.join("sounds/s.json"), &human).unwrap();
            let ev = disk.refresh(path).expect("the change is seen");
            assert!(matches!(&ev, Event::Unreadable { .. }), "{ev:?}");
            assert!(disk.still_current(&ev));
            // An agent's newer text waits: the human's bytes stay.
            files.insert(path.into(), ProjectFile::new(FileKind::Sound, Arc::from("{\"b\": 2}"), 2 + i as u64, Arc::from("ann")));
            let done = disk.sync(&files, 2);
            assert_eq!(done.failed.len(), 1, "held");
            assert!(disk.held.contains_key(path) && disk.pending[path].starts_with("disk_unreadable"));
            assert_eq!(std::fs::read(root.join("sounds/s.json")).unwrap(), human, "never overwritten");
        }
        // Replacing it on purpose backs the bytes up first.
        let backup = disk.back_up_and_release(path).unwrap();
        assert_eq!(std::fs::read(&backup).unwrap().len(), MAX_FILE_BYTES + 1);
        assert!(backup.file_name().unwrap().to_string_lossy().starts_with('.'), "hidden: never mirrored");
        assert_eq!(disk.sync(&files, 3).changed, vec![path.to_owned()]);
        assert_eq!(std::fs::read_to_string(root.join("sounds/s.json")).unwrap(), "{\"b\": 2}");
        // Review round 2: a held file is released when the disk is put back to the store's text.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.join("sounds/s.json"), b"{\"caf\xe9\": 1}").unwrap();
        let Some(Event::Unreadable { len, mtime, why, .. }) = disk.refresh(path) else { panic!("held again") };
        disk.held.insert(path.into(), (len, mtime, why)); // what the engine's import does
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.join("sounds/s.json"), "{\"b\": 2}").unwrap();
        assert!(disk.refresh(path).is_none(), "the store's own text: nothing to import");
        assert!(!disk.held.contains_key(path), "released");
        // The same through a sync that finds the disk already holding the store's newer text.
        files.insert(path.into(), ProjectFile::new(FileKind::Sound, Arc::from("{\"c\": 3}"), 9, Arc::from("ann")));
        disk.held.insert(path.into(), (0, None, "x".into()));
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.join("sounds/s.json"), "{\"c\": 3}").unwrap();
        let done = disk.sync(&files, 6);
        assert!(done.failed.is_empty() && !disk.held.contains_key(path) && !disk.pending.contains_key(path), "{done:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review finding: writes into a linked folder were committed, then pending forever; deletes
    /// didn't check links at all.
    #[cfg(windows)]
    #[test]
    fn linked_folders_are_refused() {
        let root = scratch("links");
        let outside = scratch("links-outside");
        std::fs::create_dir_all(root.join("recipes")).unwrap();
        let made = std::process::Command::new("cmd").args(["/c", "mklink", "/J"]).arg(root.join("jx")).arg(&outside).output();
        if !made.is_ok_and(|o| o.status.success()) {
            eprintln!("skipped: can't make a junction here");
            return;
        }
        assert!(check_inside(&root, &root.join("jx").join("recipes")).is_err());
        assert!(check_inside(&root, &root.join("recipes")).is_ok());
        assert!(check_inside(&root, &root.join("new").join("deeper")).is_ok(), "folders yet to be made are fine");
        // A delete through the link is refused like a write.
        std::fs::write(outside.join("t.json"), "{}").unwrap();
        let mut disk = Disk::default();
        disk.mount(Mount { name: ASSETS.into(), root: root.clone(), kind: MountKind::Assets });
        std::fs::create_dir_all(outside.join("recipes")).unwrap();
        std::fs::write(outside.join("recipes/t.json"), "{}").unwrap();
        let (len, mtime) = stat(&outside.join("recipes/t.json")).unwrap();
        disk.stamp("assets/jx/recipes/t.json", Stamp { len, mtime, hash: text_hash("{}") });
        let done = disk.sync(&BTreeMap::new(), 1);
        assert!(outside.join("recipes/t.json").exists(), "not deleted through the link: {done:?}");
        assert_eq!(done.failed.len(), 1, "{done:?}");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn sync_writes_atomically_deletes_and_never_overwrites_an_outside_edit() {
        let root = scratch("sync");
        std::fs::create_dir_all(root.join("ui")).unwrap();
        std::fs::write(root.join("ui/hud.json"), "{\"a\": 1}").unwrap();
        let mut disk = Disk::default();
        let found = disk.mount(Mount { name: ASSETS.into(), root: root.clone(), kind: MountKind::Assets });
        let mut files = BTreeMap::new();
        for (p, k, t, s) in found {
            disk.stamp(&p, s);
            files.insert(p, ProjectFile::new(k, t, 1, Arc::from("disk")));
        }
        assert!(disk.sync(&files, 1).changed.is_empty(), "nothing to do");
        // A new version and a new file are written; the folder is made.
        files.insert("assets/ui/hud.json".into(), ProjectFile::new(FileKind::UiScreen, Arc::from("{\"a\": 2}"), 2, Arc::from("ann")));
        files.insert("assets/sounds/s.json".into(), ProjectFile::new(FileKind::Sound, Arc::from("{}"), 1, Arc::from("ann")));
        let done = disk.sync(&files, 2);
        assert_eq!(done.changed.len(), 2);
        assert_eq!(std::fs::read_to_string(root.join("ui/hud.json")).unwrap(), "{\"a\": 2}");
        assert!(std::fs::read_dir(root.join("ui")).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with('.')), "no temporary file left");
        // Someone edits the file outside: the next write doesn't overwrite it, it comes back to import.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.join("ui/hud.json"), "{\"a\": 99, \"human\": true}").unwrap();
        files.insert("assets/ui/hud.json".into(), ProjectFile::new(FileKind::UiScreen, Arc::from("{\"a\": 3}"), 3, Arc::from("ann")));
        let done = disk.sync(&files, 3);
        assert_eq!(done.raced.len(), 1);
        assert_eq!(&*done.raced[0].1, "{\"a\": 99, \"human\": true}");
        assert_eq!(std::fs::read_to_string(root.join("ui/hud.json")).unwrap(), "{\"a\": 99, \"human\": true}");
        // Deleting from the store deletes on disk.
        files.remove("assets/sounds/s.json");
        let done = disk.sync(&files, 4);
        assert!(done.changed.contains(&"assets/sounds/s.json".to_owned()));
        assert!(!root.join("sounds/s.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}

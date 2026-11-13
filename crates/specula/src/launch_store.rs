//! The project list and the markers, on disk (docs/design/LAUNCH.md). `launch_list` is the
//! data; this is where it is kept: `projects.json` in the engine's per-user folder, and
//! `specula.project.json` at each project's root.
//!
//! Called from start-up, shutdown and the command line: never from a frame. Every file read
//! is bounded; the list is written whole to a temporary file and renamed over the old one, so
//! a crash mid-write leaves the old list. A list file that isn't one is kept aside, never
//! overwritten.

use crate::launch_list::{Marker, Project, ProjectList, LIST_FILE, MARKER, MAX_LIST_BYTES, MAX_MARKER_BYTES};
use std::path::{Path, PathBuf};

/// Where the list is kept.
#[derive(Clone, Debug)]
pub struct Store {
    /// The engine's per-user folder (`--settings DIR`, or the operating system's).
    pub dir: PathBuf,
}

/// Reads a file of at most `max` bytes as text. `Ok(None)`: there is no such file.
fn read_small(path: &Path, max: u64) -> Result<Option<String>, String> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} can't be read ({e})", path.display())),
    };
    if !meta.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    if meta.len() > max {
        return Err(format!("{} is {} bytes; at most {max} are read", path.display(), meta.len()));
    }
    std::fs::read_to_string(path).map(Some).map_err(|e| format!("{} can't be read ({e})", path.display()))
}

/// An error of this module as `std::io::Error` (its callers report words).
fn io(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

/// Makes a folder through the disk book (docs/design/DISK.md: every write is admitted there).
pub(crate) fn make_dir(dir: &Path) -> std::io::Result<()> {
    crate::disk::book().make_dir(crate::disk::Category::Settings, dir, crate::disk::Need::Essential).map_err(io)
}

/// Writes a small file of the launcher's through the disk book.
pub(crate) fn write_small(path: &Path, text: &str) -> std::io::Result<()> {
    crate::disk::book().write_synced(crate::disk::Category::Settings, "launch", path, text.as_bytes(), crate::disk::Need::Essential).map_err(io)
}

/// Removes a file of the launcher's own that sits directly in `root` (a missing one is fine).
/// Through the disk module's own remover: never through a link, never outside the folder.
pub(crate) fn remove_inside(root: &Path, path: &Path) {
    // (It compares against the folder as the system spells it.)
    if let (Ok(root), Some(name)) = (root.canonicalize(), path.file_name()) {
        let _ = crate::disk::remove_file_inside(&root, &root.join(name));
    }
}

/// Whether two paths are one folder: as the list compares them, or as the system resolves them
/// (a junction, a short name, `..`: review R2-1). Asks the disk: the command line's own process
/// and workers only.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    crate::launch_list::same_path(a, b) || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if crate::launch_list::same_path(&x, &y))
}

/// Makes a small file of the launcher's that must not be there yet: `Ok(false)` when it is
/// (another command made it first), so two commands never overwrite each other's request
/// (review R2-5). Admitted by the disk book like every write.
pub(crate) fn create_small(path: &Path, text: &str) -> std::io::Result<bool> {
    use std::io::Write as _;
    crate::disk::book().admit(crate::disk::Category::Settings, text.len() as u64, crate::disk::Need::Essential).map_err(io)?;
    match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            file.write_all(text.as_bytes())?;
            crate::disk::book().wrote(crate::disk::Category::Settings, "launch", text.len() as i64, 1);
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e),
    }
}

/// Writes `text` to `path` through a temporary file beside it: a reader sees the old file or
/// the new one, never half of one.
pub(crate) fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    write_small(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        if let Some(dir) = tmp.parent() {
            remove_inside(dir, &tmp);
        }
    })
}

impl Store {
    pub fn new(dir: PathBuf) -> Store {
        Store { dir }
    }

    pub fn list_path(&self) -> PathBuf {
        self.dir.join(LIST_FILE)
    }

    /// The list, and what was wrong with its file if it could not be read as one (the list is
    /// then empty, and [`Store::save`] keeps the unreadable file aside first). No file yet: an
    /// empty list, nothing wrong.
    pub fn load(&self) -> (ProjectList, Option<String>) {
        match read_small(&self.list_path(), MAX_LIST_BYTES) {
            Ok(None) => (ProjectList::default(), None),
            Ok(Some(text)) => match ProjectList::parse(&text) {
                Ok((list, 0)) => (list, None),
                Ok((list, skipped)) => (list, Some(format!("{skipped} entr{} of {} were not projects and were left out", if skipped == 1 { "y" } else { "ies" }, self.list_path().display()))),
                Err(why) => (ProjectList::default(), Some(format!("{} is not a project list: {why}", self.list_path().display()))),
            },
            Err(why) => (ProjectList::default(), Some(why)),
        }
    }

    /// Writes the list. A file that is there and isn't a project list is renamed
    /// `projects.json.unreadable` first (the newest such copy is kept).
    pub fn save(&self, list: &ProjectList) -> std::io::Result<()> {
        make_dir(&self.dir)?;
        let path = self.list_path();
        if let Ok(Some(text)) = read_small(&path, MAX_LIST_BYTES) {
            if ProjectList::parse(&text).is_err() {
                let _ = std::fs::rename(&path, self.dir.join(format!("{LIST_FILE}.unreadable")));
            }
        } else if path.exists() {
            // Too large, or not a file's text: not ours to overwrite either.
            let _ = std::fs::rename(&path, self.dir.join(format!("{LIST_FILE}.unreadable")));
        }
        write_whole(&path, &list.to_text())
    }

    /// A project was opened: it is first in the list now. (Two engines that open projects in
    /// the same instant each read, change and write the list: the later write wins and the
    /// other's project is listed again when it is next opened.)
    pub fn opened(&self, project: Project) -> std::io::Result<()> {
        let (mut list, _) = self.load();
        list.opened(project);
        self.save(&list)
    }

    /// The world a project had open when its engine stopped.
    pub fn closed(&self, folder: &Path, world: Option<&str>) -> std::io::Result<()> {
        let (mut list, _) = self.load();
        if list.set_world(folder, world) {
            self.save(&list)?;
        }
        Ok(())
    }

    /// Takes a project off the list. Whether it was listed.
    pub fn forget(&self, folder: &Path) -> std::io::Result<bool> {
        let (mut list, _) = self.load();
        let was = list.forget(folder);
        if was {
            self.save(&list)?;
        }
        Ok(was)
    }
}

/// A folder's marker: `Ok(None)` when it has none (not a project yet).
pub fn read_marker(folder: &Path) -> Result<Option<Marker>, String> {
    match read_small(&folder.join(MARKER), MAX_MARKER_BYTES)? {
        None => Ok(None),
        Some(text) => Marker::parse(&text).map(Some).map_err(|why| format!("{} is not a project marker: {why}", folder.join(MARKER).display())),
    }
}

pub fn write_marker(folder: &Path, marker: &Marker) -> std::io::Result<()> {
    write_whole(&folder.join(MARKER), &marker.to_text())
}

/// The name a folder goes by when it is opened as a project for the first time: its own last
/// component when that is a valid name, else "project".
pub fn name_for(folder: &Path) -> String {
    folder.file_name().and_then(|n| n.to_str()).filter(|n| crate::launch_list::valid_name(n)).unwrap_or("project").to_owned()
}

/// Makes a new project: the folder `parent/name` (which must not exist, or be an empty
/// folder), an `assets` folder in it, and its marker. Returns the project's folder.
pub fn create(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if !crate::launch_list::valid_name(name) {
        return Err(format!("'{name}' is not a project name"));
    }
    let folder = parent.join(name);
    match std::fs::read_dir(&folder) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(format!("{} is there already and is not empty: open it as a project (specula open \"{}\"), or pick another name", folder.display(), folder.display()));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{} can't be used ({e})", folder.display())),
    }
    let made = make_dir(&folder.join("assets")).and_then(|()| write_marker(&folder, &Marker { name: name.to_owned(), assets: Some("assets".to_owned()) }));
    made.map_err(|e| format!("{} could not be made ({e})", folder.display()))?;
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-launch-store-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn project(path: &Path, name: &str, at: u64) -> Project {
        Project { path: path.to_owned(), name: name.into(), exe: None, last_world: None, last_opened: at, version: "0.3.0".into() }
    }

    #[test]
    fn the_list_starts_empty_and_keeps_the_last_opened_first_across_reads() {
        let dir = scratch("order");
        let store = Store::new(dir.join("user"));
        assert_eq!(store.load(), (ProjectList::default(), None), "never used: no file, an empty list, nothing wrong");
        store.opened(project(&dir.join("crypt"), "crypt", 10)).unwrap();
        store.opened(project(&dir.join("frost"), "frost", 20)).unwrap();
        store.opened(project(&dir.join("crypt"), "crypt", 30)).unwrap();
        let (list, problem) = store.load();
        assert_eq!(problem, None);
        assert_eq!(list.projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["crypt", "frost"]);
        // The world it had open is kept when its engine stops, and comes back with the next open.
        store.closed(&dir.join("frost"), Some("dawn")).unwrap();
        store.opened(project(&dir.join("frost"), "frost", 40)).unwrap();
        assert_eq!(store.load().0.last().map(|p| (p.name.clone(), p.last_world.clone())), Some(("frost".into(), Some("dawn".into()))));
        assert!(store.forget(&dir.join("crypt")).unwrap() && !store.forget(&dir.join("crypt")).unwrap());
        assert_eq!(store.load().0.projects.len(), 1);
        // No temporary file is left beside the list.
        let left: Vec<String> = std::fs::read_dir(&store.dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left, [LIST_FILE]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_list_file_that_is_not_one_is_kept_aside_never_overwritten() {
        let dir = scratch("aside");
        let store = Store::new(dir.clone());
        std::fs::write(store.list_path(), "my own notes, not a list").unwrap();
        let (list, problem) = store.load();
        assert!(list.projects.is_empty() && problem.is_some_and(|p| p.contains("is not a project list")));
        store.opened(project(&dir.join("crypt"), "crypt", 10)).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(format!("{LIST_FILE}.unreadable"))).unwrap(), "my own notes, not a list");
        assert_eq!(store.load().0.projects.len(), 1);
        // Entries that aren't projects are left out and said; the rest is read.
        std::fs::write(store.list_path(), r#"{"schema": "specula.projects.v1", "projects": [{"path": "C:/x", "name": "x"}, 5]}"#).unwrap();
        let (list, problem) = store.load();
        assert!(list.projects.len() == 1 && problem.is_some_and(|p| p.contains("1 entry")));
        // A file too large to be the engine's is not read (and is kept aside at the next save).
        std::fs::write(store.list_path(), " ".repeat(MAX_LIST_BYTES as usize + 1)).unwrap();
        assert!(store.load().1.is_some_and(|p| p.contains("at most")));
        store.opened(project(&dir.join("frost"), "frost", 20)).unwrap();
        assert_eq!(store.load().0.projects.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_project_is_a_folder_with_its_marker_and_never_takes_a_folder_in_use() {
        let dir = scratch("create");
        let folder = create(&dir, "crypt").unwrap();
        assert_eq!(folder, dir.join("crypt"));
        assert!(folder.join("assets").is_dir());
        assert_eq!(read_marker(&folder).unwrap(), Some(Marker { name: "crypt".into(), assets: Some("assets".into()) }));
        assert!(create(&dir, "crypt").unwrap_err().contains("is not empty"), "a folder in use is never taken");
        std::fs::create_dir_all(dir.join("empty")).unwrap();
        assert!(create(&dir, "empty").is_ok(), "an empty folder is fine");
        assert!(create(&dir, "a/b").unwrap_err().contains("not a project name"));
        // A folder with no marker is not a project yet; one whose marker isn't one says so.
        std::fs::create_dir_all(dir.join("plain")).unwrap();
        assert_eq!(read_marker(&dir.join("plain")), Ok(None));
        std::fs::write(dir.join("plain").join(MARKER), "{}").unwrap();
        assert!(read_marker(&dir.join("plain")).unwrap_err().contains("is not a project marker"));
        assert_eq!(name_for(&dir.join("plain")), "plain");
        assert_eq!(name_for(Path::new("C:/")), "project");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! The project list (docs/design/LAUNCH.md): the projects this user has worked on, most recent
//! first, kept in their per-user folder (`projects.json`). The engine moves a project to the top
//! when it opens it; the start screen and `specula projects` read it; "resume the last one" is
//! its first entry.
//!
//! A project is a folder with a marker file ([`MARKER`]) at its root. The list is the user's
//! own: it is read by the local command line and the start screen, never served as a tool (a
//! connected agent would read the names of the owner's other folders).
//!
//! Everything here is plain data: no clock, no disk (the callers read and write the files and
//! say what time it is), so the order and the limits are tested without either.

use specula_core::Json;
use std::path::{Path, PathBuf};

/// Projects the list holds (the oldest fall off).
pub const MAX_PROJECTS: usize = 64;
/// The list file's largest size read (a larger one is not this engine's).
pub const MAX_LIST_BYTES: u64 = 256 * 1024;
/// A marker file's largest size read.
pub const MAX_MARKER_BYTES: u64 = 16 * 1024;
/// A project's name, characters.
pub const MAX_NAME_CHARS: usize = 64;
/// A path kept in the list or a marker, bytes.
pub const MAX_PATH_BYTES: usize = 1024;
/// The marker file at a project's root.
pub const MARKER: &str = "specula.project.json";
/// The list's file name in the per-user folder.
pub const LIST_FILE: &str = "projects.json";
const LIST_SCHEMA: &str = "specula.projects.v1";
const MARKER_SCHEMA: &str = "specula.project.v1";

/// One project of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    /// Its folder (absolute).
    pub path: PathBuf,
    pub name: String,
    /// The executable that opened it last (the stock engine, or a game's own), so "resume"
    /// starts the same one.
    pub exe: Option<PathBuf>,
    /// The world it had open (`--world`), to resume.
    pub last_world: Option<String>,
    /// When it was last opened: seconds since the Unix epoch, UTC.
    pub last_opened: u64,
    /// The engine version that opened it.
    pub version: String,
}

/// The list: most recently opened first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectList {
    pub projects: Vec<Project>,
}

/// What a name or a path picked from the list.
#[derive(Clone, Debug, PartialEq)]
pub enum Found<'a> {
    One(&'a Project),
    /// No project is called that (nor is at that path).
    None,
    /// Several projects share the name: their folders, most recent first.
    Several(Vec<&'a Path>),
}

/// A name for a project: 1..=[`MAX_NAME_CHARS`] characters, trimmed, no control characters and
/// none a path uses (so a name is never mistaken for a path, and is safe as a folder's name on
/// every system: Windows drops a trailing dot, so `crypt.` would be listed as one folder and
/// made as another, and keeps `CON`, `NUL`, `COM1`... for devices, with any extension).
pub fn valid_name(name: &str) -> bool {
    let n = name.chars().count();
    (1..=MAX_NAME_CHARS).contains(&n)
        && name.trim() == name
        && !name.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        && !name.ends_with('.')
        && !device_name(name)
}

/// Whether Windows reads `name` as a device, whatever its extension (`NUL`, `con.txt`, `COM3`).
fn device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let numbered = |prefix: &str| stem.strip_prefix(prefix).is_some_and(|d| d.len() == 1 && d.bytes().all(|b| b.is_ascii_digit()));
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered("COM") || numbered("LPT")
}

/// A path as the list compares it: separators made one kind, a trailing one dropped, and on
/// Windows (where `C:\Games\Crypt` and `c:\games\crypt` are one folder) lower case.
pub(crate) fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.to_owned()
    }
}

/// Whether two paths name the same folder as the list sees it (no disk is asked).
pub fn same_path(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

fn text(j: &Json, field: &str, max_bytes: usize) -> Option<String> {
    j.get(field).and_then(Json::as_str).filter(|s| !s.is_empty() && s.len() <= max_bytes).map(str::to_owned)
}

impl Project {
    fn parse(j: &Json) -> Option<Project> {
        let path = PathBuf::from(text(j, "path", MAX_PATH_BYTES)?);
        let name = text(j, "name", MAX_NAME_CHARS * 4).filter(|n| valid_name(n))?;
        Some(Project {
            path,
            name,
            exe: text(j, "exe", MAX_PATH_BYTES).map(PathBuf::from),
            last_world: text(j, "last_world", 256),
            last_opened: j.get("last_opened_utc").and_then(Json::as_u64).unwrap_or(0),
            version: text(j, "version", 64).unwrap_or_default(),
        })
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("path", Json::from(self.path.to_string_lossy().as_ref())), ("name", Json::from(self.name.as_str()))];
        if let Some(e) = &self.exe {
            f.push(("exe", Json::from(e.to_string_lossy().as_ref())));
        }
        if let Some(w) = &self.last_world {
            f.push(("last_world", Json::from(w.as_str())));
        }
        f.push(("last_opened_utc", Json::from(self.last_opened)));
        f.push(("version", Json::from(self.version.as_str())));
        Json::obj(f)
    }
}

impl ProjectList {
    /// Reads a list file's text. Entries that don't read as a project are left out (and counted:
    /// the second value), a folder listed twice keeps its first entry, and at most
    /// [`MAX_PROJECTS`] are kept. An error means the file isn't a project list at all: the
    /// caller keeps it aside and starts an empty list, never overwriting what it couldn't read.
    pub fn parse(text: &str) -> Result<(ProjectList, usize), String> {
        let j = Json::parse(text).map_err(|e| format!("not JSON ({e})"))?;
        if j.get("schema").and_then(Json::as_str) != Some(LIST_SCHEMA) {
            return Err(format!("its \"schema\" is not \"{LIST_SCHEMA}\""));
        }
        let entries = j.get("projects").and_then(Json::as_array).ok_or("it has no \"projects\" list")?;
        let mut list = ProjectList::default();
        let mut skipped = 0;
        for e in entries {
            match Project::parse(e) {
                Some(p) if list.projects.len() < MAX_PROJECTS && !list.projects.iter().any(|q| same_path(&q.path, &p.path)) => list.projects.push(p),
                _ => skipped += 1,
            }
        }
        Ok((list, skipped))
    }

    pub fn to_text(&self) -> String {
        Json::obj([("schema", Json::from(LIST_SCHEMA)), ("projects", Json::arr(self.projects.iter().map(Project::to_json)))]).to_pretty()
    }

    /// A project was opened: it is first now. An entry for the same folder is replaced (its
    /// last world is kept when `p` names none); the list stays within [`MAX_PROJECTS`].
    pub fn opened(&mut self, mut p: Project) {
        if let Some(at) = self.projects.iter().position(|q| same_path(&q.path, &p.path)) {
            let old = self.projects.remove(at);
            if p.last_world.is_none() {
                p.last_world = old.last_world;
            }
        }
        self.projects.insert(0, p);
        self.projects.truncate(MAX_PROJECTS);
    }

    /// The world a project had open when its engine stopped. Whether the project is listed.
    pub fn set_world(&mut self, path: &Path, world: Option<&str>) -> bool {
        match self.projects.iter_mut().find(|q| same_path(&q.path, path)) {
            Some(q) => {
                q.last_world = world.map(str::to_owned);
                true
            }
            None => false,
        }
    }

    /// Takes a project off the list (its folder is not touched). Whether it was listed.
    pub fn forget(&mut self, path: &Path) -> bool {
        let before = self.projects.len();
        self.projects.retain(|q| !same_path(&q.path, path));
        self.projects.len() != before
    }

    /// The project last worked on: the first of the list.
    pub fn last(&self) -> Option<&Project> {
        self.projects.first()
    }

    /// The project a word means: its folder (when the word is a listed path), else its name
    /// (case-insensitively). Names are not unique: two folders may both be called "demo".
    pub fn find(&self, word: &str) -> Found<'_> {
        if let Some(p) = self.projects.iter().find(|q| same_path(&q.path, Path::new(word))) {
            return Found::One(p);
        }
        let named: Vec<&Project> = self.projects.iter().filter(|q| q.name.to_lowercase() == word.to_lowercase()).collect();
        match named.len() {
            0 => Found::None,
            1 => Found::One(named[0]),
            _ => Found::Several(named.iter().map(|q| q.path.as_path()).collect()),
        }
    }
}

/// A project's marker file: its name, and where its assets are (a folder inside it; none: the
/// project's own folder, as an engine started there has always read them).
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub name: String,
    pub assets: Option<String>,
}

impl Marker {
    pub fn parse(text: &str) -> Result<Marker, String> {
        let j = Json::parse(text).map_err(|e| format!("not JSON ({e})"))?;
        if j.get("schema").and_then(Json::as_str) != Some(MARKER_SCHEMA) {
            return Err(format!("its \"schema\" is not \"{MARKER_SCHEMA}\""));
        }
        let name = j.get("name").and_then(Json::as_str).filter(|n| valid_name(n)).ok_or(format!("its \"name\" must be 1..{MAX_NAME_CHARS} characters, none of / \\ : * ? \" < > |"))?;
        let assets = match j.get("assets") {
            None => None,
            Some(a) => Some(a.as_str().filter(|a| inside(a)).ok_or("its \"assets\" must be a folder inside the project (a relative path with no ..)")?.to_owned()),
        };
        Ok(Marker { name: name.to_owned(), assets })
    }

    pub fn to_text(&self) -> String {
        let mut f = vec![("schema", Json::from(MARKER_SCHEMA)), ("name", Json::from(self.name.as_str()))];
        if let Some(a) = &self.assets {
            f.push(("assets", Json::from(a.as_str())));
        }
        Json::obj(f).to_pretty()
    }
}

/// Whether a marker's path stays inside the project: relative, no `..`, no drive or root, and
/// short.
fn inside(rel: &str) -> bool {
    !rel.is_empty() && rel.len() <= 256 && !rel.contains(':') && !rel.starts_with(['/', '\\']) && rel.split(['/', '\\']).all(|part| !part.is_empty() && part != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review R1-11: a name is a folder's name on every system.
    #[test]
    fn a_name_no_folder_can_keep_is_refused() {
        for good in ["crypt", "Frozen Wasteland", "v1.2", ".hidden", "console", "com", "COM10", "nulls", "日本"] {
            assert!(valid_name(good), "{good}");
        }
        for bad in ["", " crypt", "crypt ", "crypt.", "crypt..", ".", "..", "a/b", "a\\b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a|b", "a\tb", "CON", "con", "Nul", "nul.txt", "PRN", "aux.json", "COM1", "com9.dat", "LPT3"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        assert!(!valid_name(&"x".repeat(MAX_NAME_CHARS + 1)) && valid_name(&"x".repeat(MAX_NAME_CHARS)));
    }

    fn project(path: &str, name: &str, at: u64) -> Project {
        Project { path: PathBuf::from(path), name: name.into(), exe: None, last_world: None, last_opened: at, version: "0.3.0".into() }
    }

    #[test]
    fn the_project_last_opened_is_first_and_a_folder_is_listed_once() {
        let mut list = ProjectList::default();
        assert!(list.last().is_none(), "nothing worked on yet: an empty list");
        list.opened(project("C:/games/crypt", "crypt", 10));
        list.opened(project("C:/games/frost", "frost", 20));
        list.opened(project("C:/games/ember", "ember", 30));
        let names = |l: &ProjectList| l.projects.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&list), ["ember", "frost", "crypt"]);
        // Opening an older one again moves it to the top; it is still listed once.
        list.opened(project("C:/games/crypt/", "crypt", 40));
        assert_eq!(names(&list), ["crypt", "ember", "frost"]);
        assert_eq!(list.last().map(|p| p.last_opened), Some(40));
        // Its last world is kept when the engine that opened it named none, and replaced when
        // it names one.
        assert!(list.set_world(Path::new("C:/games/crypt"), Some("night")));
        list.opened(project("C:/games/frost", "frost", 50));
        list.opened(project("C:/games/crypt", "crypt", 60));
        assert_eq!(list.last().and_then(|p| p.last_world.clone()), Some("night".into()));
        let mut dawn = project("C:/games/crypt", "crypt", 70);
        dawn.last_world = Some("dawn".into());
        list.opened(dawn);
        assert_eq!(list.last().and_then(|p| p.last_world.clone()), Some("dawn".into()));
        assert!(!list.set_world(Path::new("C:/games/nowhere"), None));
        // Forgetting takes it off the list.
        assert!(list.forget(Path::new("C:/games/ember")) && !list.forget(Path::new("C:/games/ember")));
        assert_eq!(names(&list), ["crypt", "frost"]);
    }

    #[test]
    fn the_list_holds_its_limit_and_the_oldest_fall_off() {
        let mut list = ProjectList::default();
        for i in 0..MAX_PROJECTS + 5 {
            list.opened(project(&format!("C:/p/{i}"), &format!("p{i}"), i as u64));
        }
        assert_eq!(list.projects.len(), MAX_PROJECTS);
        assert_eq!(list.projects[0].name, format!("p{}", MAX_PROJECTS + 4));
        assert_eq!(list.projects[MAX_PROJECTS - 1].name, "p5", "the five oldest are gone");
    }

    #[test]
    fn paths_are_one_folder_whatever_their_slashes_and_on_windows_their_case() {
        assert!(same_path(Path::new("C:\\Games\\Crypt\\"), Path::new("C:/Games/Crypt")));
        assert_eq!(same_path(Path::new("C:/Games/Crypt"), Path::new("c:/games/crypt")), cfg!(windows));
        assert!(!same_path(Path::new("C:/Games/Crypt"), Path::new("C:/Games/Crypt2")));
    }

    #[test]
    fn a_project_is_found_by_its_path_or_its_name_and_shared_names_are_said() {
        let mut list = ProjectList::default();
        list.opened(project("C:/a/demo", "demo", 1));
        list.opened(project("C:/b/demo", "demo", 2));
        list.opened(project("C:/c/crypt", "Crypt", 3));
        assert!(matches!(list.find("crypt"), Found::One(p) if p.path == Path::new("C:/c/crypt")), "by name, whatever its case");
        assert!(matches!(list.find("C:/a/demo"), Found::One(p) if p.last_opened == 1), "by path");
        assert_eq!(list.find("demo"), Found::Several(vec![Path::new("C:/b/demo"), Path::new("C:/a/demo")]));
        assert_eq!(list.find("frost"), Found::None);
    }

    #[test]
    fn the_list_file_round_trips_and_reads_only_what_is_a_project() {
        let mut list = ProjectList::default();
        let mut p = project("C:/games/crypt", "crypt", 100);
        p.exe = Some(PathBuf::from("C:/specula/specula.exe"));
        p.last_world = Some("night".into());
        list.opened(p);
        list.opened(project("C:/games/frost", "frost", 200));
        let (back, skipped) = ProjectList::parse(&list.to_text()).unwrap();
        assert_eq!((back, skipped), (list.clone(), 0));
        // Entries that aren't projects are left out and counted; a folder listed twice is kept
        // once; more than the limit are not kept.
        let odd = r#"{"schema": "specula.projects.v1", "projects": [
            {"path": "C:/x", "name": "x", "last_opened_utc": 5, "version": "1"},
            {"path": "C:/x/", "name": "again"},
            {"name": "no path"}, {"path": "C:/y", "name": "bad/name"}, {"path": "", "name": "y"}, 7, null,
            {"path": "C:/z", "name": "z"}]}"#;
        let (list, skipped) = ProjectList::parse(odd).unwrap();
        assert_eq!(list.projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["x", "z"]);
        assert_eq!(skipped, 6);
        let many = format!(r#"{{"schema": "specula.projects.v1", "projects": [{}]}}"#, (0..80).map(|i| format!(r#"{{"path": "C:/p/{i}", "name": "p{i}"}}"#)).collect::<Vec<_>>().join(","));
        let (list, skipped) = ProjectList::parse(&many).unwrap();
        assert_eq!((list.projects.len(), skipped), (MAX_PROJECTS, 80 - MAX_PROJECTS));
        // What isn't a project list is refused (the caller keeps the file aside).
        assert!(ProjectList::parse("not json").unwrap_err().contains("not JSON"));
        assert!(ProjectList::parse(r#"{"schema": "other", "projects": []}"#).unwrap_err().contains("schema"));
        assert!(ProjectList::parse(r#"{"schema": "specula.projects.v1"}"#).unwrap_err().contains("projects"));
        assert_eq!(ProjectList::parse(r#"{"schema": "specula.projects.v1", "projects": []}"#).unwrap().0, ProjectList::default());
    }

    #[test]
    fn names_are_short_and_never_a_path() {
        for ok in ["crypt", "Frozen Wasteland", "map-2_v1.0", "ö", &"x".repeat(MAX_NAME_CHARS)] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in ["", " crypt", "crypt ", "a/b", "a\\b", "C:", "what?", "a\nb", ".", "..", &"x".repeat(MAX_NAME_CHARS + 1)] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_marker_names_the_project_and_keeps_its_assets_inside_it() {
        let m = Marker { name: "crypt".into(), assets: Some("assets".into()) };
        assert_eq!(Marker::parse(&m.to_text()).unwrap(), m);
        let plain = Marker::parse(r#"{"schema": "specula.project.v1", "name": "old"}"#).unwrap();
        assert_eq!(plain, Marker { name: "old".into(), assets: None });
        let bad = |t: &str| Marker::parse(t).unwrap_err();
        assert!(bad("{").contains("not JSON"));
        assert!(bad(r#"{"name": "x"}"#).contains("schema"));
        assert!(bad(r#"{"schema": "specula.project.v1", "name": "a/b"}"#).contains("name"));
        for outside in ["..", "../other", "a/../../b", "/abs", "\\abs", "C:/abs", "", "a//b"] {
            let t = format!(r#"{{"schema": "specula.project.v1", "name": "x", "assets": {}}}"#, Json::from(outside));
            assert!(bad(&t).contains("inside the project"), "{outside:?}");
        }
        assert!(Marker::parse(r#"{"schema": "specula.project.v1", "name": "x", "assets": "data/assets"}"#).is_ok());
    }
}

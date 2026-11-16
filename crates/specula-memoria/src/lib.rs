//! Specula Memoria (*memoria*: memory): what a project remembers.
//!
//! An agent that builds a game has no memory of yesterday's session, and the next agent has none
//! of this one's. In M5's measured comparison the engine cost least where a game's own documents
//! said how it was built (docs/design/LEVERAGE.md). So a project keeps notes, with the project
//! (docs/design/FIRST_HOUR.md, slice 6; owner, 2026-11-16: "notes that agents can use to note
//! about the game, both for memory and system").
//!
//! **A note is a file a person can read**: `notes/<id>.md`, a few lines of front matter and
//! text. It is a project file like any other, so it has claims, versions, a journal and undo, it
//! travels with the folder, and any editor opens it.
//!
//! ```text
//! ---
//! title: The gate has ten lives on every difficulty
//! kind: decision
//! supersedes: gate-lives
//! about: modules/gate/src/lib.rs, game.json
//! ---
//! Difficulty changes how fast runners come, never the gate's lives: a player who ...
//! ```
//!
//! **A note that replaces another says so**, and the older one stays in the folder but is no
//! longer listed (the owner's Relatum works this way): memory that only grows is not read.
//!
//! This crate is the notes' meaning: reading and writing one, which are current, finding the
//! ones about a path or a word, and the brief an agent is given at its first call. Pure: no
//! files, no clock.
#![forbid(unsafe_code)]

/// The folder under a project that holds its notes.
pub const FOLDER: &str = "notes";
/// The largest note read, in bytes, and the most notes a project's memory holds.
pub const MAX_NOTE_BYTES: usize = 16 * 1024;
pub const MAX_NOTES: usize = 1024;
/// Most characters in a title, and most paths a note is about.
pub const MAX_TITLE: usize = 120;
pub const MAX_ABOUT: usize = 16;
/// The largest brief, in characters (about 600 tokens).
pub const MAX_BRIEF: usize = 2400;

/// What kind of thing a note says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// What the game is. A project has one.
    Brief,
    /// How it is built and checked.
    How,
    /// What was decided, and why.
    Decision,
    /// What is open.
    Thread,
    /// People, places, numbers: what is so in the game.
    Fact,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Brief, Kind::How, Kind::Decision, Kind::Thread, Kind::Fact];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Brief => "brief",
            Kind::How => "how",
            Kind::Decision => "decision",
            Kind::Thread => "thread",
            Kind::Fact => "fact",
        }
    }

    pub fn named(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// What is wrong with a note, as data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteError {
    pub code: &'static str,
    pub message: String,
}

impl NoteError {
    fn new(code: &'static str, message: impl Into<String>) -> NoteError {
        NoteError { code, message: message.into() }
    }
}

impl std::fmt::Display for NoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for NoteError {}

/// One note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// Its file's name without `.md`.
    pub id: String,
    pub title: String,
    pub kind: Kind,
    /// The note this one replaces.
    pub supersedes: Option<String>,
    /// Project paths it is about (with `/`; a folder ends in `/`).
    pub about: Vec<String>,
    pub body: String,
}

/// A note's id: a lowercase letter or digit, then lowercase letters, digits, `_` and `-`, at
/// most 64.
pub fn id_ok(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) && id.len() <= 64 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// A project path a note may be about: plain names joined by `/`, none `.` or `..`; a folder
/// may end in `/`.
fn path_ok(path: &str) -> bool {
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    !trimmed.is_empty() && path.len() <= 200 && trimmed.split('/').all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains(['\\', ':']) && !part.chars().any(char::is_control))
}

/// An id made from a title: its letters and digits, lowercase, joined by `-`, at most 48.
pub fn id_from(title: &str) -> String {
    let mut id = String::new();
    for c in title.chars() {
        if id.len() >= 48 {
            break;
        }
        if c.is_ascii_alphanumeric() {
            id.push(c.to_ascii_lowercase());
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    let id = id.trim_end_matches('-').to_owned();
    if id.is_empty() {
        "note".to_owned()
    } else {
        id
    }
}

impl Note {
    /// Checks a note's parts, as [`Note::parse`] does for a file's.
    pub fn check(&self) -> Result<(), NoteError> {
        if !id_ok(&self.id) {
            return Err(NoteError::new("bad_id", "a note's id is a lowercase letter or digit, then lowercase letters, digits, _ and - (at most 64)"));
        }
        let title = self.title.trim();
        if title.is_empty() || title.chars().count() > MAX_TITLE || title.chars().any(char::is_control) {
            return Err(NoteError::new("bad_title", format!("a note's title is one line of 1 to {MAX_TITLE} characters: say the thing itself, as a sentence")));
        }
        if let Some(old) = &self.supersedes {
            if !id_ok(old) || *old == self.id {
                return Err(NoteError::new("bad_supersedes", "\"supersedes\" is the id of another note, the one this replaces"));
            }
        }
        if self.about.len() > MAX_ABOUT {
            return Err(NoteError::new("bad_about", format!("a note is about at most {MAX_ABOUT} paths")));
        }
        if let Some(bad) = self.about.iter().find(|p| !path_ok(p)) {
            return Err(NoteError::new("bad_about", format!("\"{bad}\" is not a project path (folders and files with /, such as modules/gate/src/lib.rs; a folder may end in /)")));
        }
        if self.to_text().len() > MAX_NOTE_BYTES {
            return Err(NoteError::new("too_big", format!("a note is at most {} KB: a longer one is two notes", MAX_NOTE_BYTES / 1024)));
        }
        Ok(())
    }

    /// Reads a note from its file's text. `id`: the file's name without `.md`.
    pub fn parse(id: &str, text: &str) -> Result<Note, NoteError> {
        if text.len() > MAX_NOTE_BYTES {
            return Err(NoteError::new("too_big", format!("a note is at most {} KB", MAX_NOTE_BYTES / 1024)));
        }
        let text = text.replace("\r\n", "\n");
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let no_front = || NoteError::new("no_front_matter", "a note starts with front matter between two lines of ---: title, kind, and optionally supersedes and about");
        let rest = text.strip_prefix("---\n").ok_or_else(no_front)?;
        let (front, body) = match rest.split_once("\n---\n") {
            Some((front, body)) => (front, body),
            None => (rest.strip_suffix("\n---").ok_or_else(no_front)?, ""),
        };
        let mut note = Note { id: id.to_owned(), title: String::new(), kind: Kind::Fact, supersedes: None, about: Vec::new(), body: body.trim_matches('\n').to_owned() };
        let (mut has_title, mut has_kind) = (false, false);
        for line in front.lines().filter(|l| !l.trim().is_empty()) {
            let (key, value) = line.split_once(':').map(|(k, v)| (k.trim(), v.trim())).ok_or_else(|| NoteError::new("bad_front_matter", format!("a front matter line is `key: value` (got \"{line}\")")))?;
            match key {
                "title" => {
                    note.title = value.to_owned();
                    has_title = true;
                }
                "kind" => {
                    note.kind = Kind::named(value).ok_or_else(|| NoteError::new("bad_kind", format!("a note's kind is one of: {}", Kind::ALL.map(Kind::name).join(", "))))?;
                    has_kind = true;
                }
                "supersedes" => note.supersedes = Some(value.to_owned()),
                "about" => note.about = value.split(',').map(str::trim).filter(|p| !p.is_empty()).map(str::to_owned).collect(),
                other => return Err(NoteError::new("bad_front_matter", format!("\"{other}\" is not a front matter key; there are: title, kind, supersedes, about"))),
            }
        }
        if !has_title || !has_kind {
            return Err(NoteError::new("bad_front_matter", "a note's front matter has a title and a kind"));
        }
        note.check()?;
        Ok(note)
    }

    /// The note as its file's text.
    pub fn to_text(&self) -> String {
        let mut text = format!("---\ntitle: {}\nkind: {}\n", self.title.trim(), self.kind.name());
        if let Some(old) = &self.supersedes {
            text.push_str(&format!("supersedes: {old}\n"));
        }
        if !self.about.is_empty() {
            text.push_str(&format!("about: {}\n", self.about.join(", ")));
        }
        text.push_str("---\n");
        if !self.body.is_empty() {
            text.push_str(self.body.trim_matches('\n'));
            text.push('\n');
        }
        text
    }

    /// The note's path below the project.
    pub fn path(&self) -> String {
        format!("{FOLDER}/{}.md", self.id)
    }

    /// Whether the note is about `path`: it names the path, or a folder the path is in.
    pub fn is_about(&self, path: &str) -> bool {
        self.about.iter().any(|a| a == path || (a.ends_with('/') && path.starts_with(a.as_str())) || path.strip_prefix(a.as_str()).is_some_and(|rest| rest.starts_with('/')))
    }
}

/// A project's notes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notes {
    notes: Vec<Note>,
}

/// The first `max` characters of `text`, cut at a word and marked when something was cut.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max.saturating_sub(4)).collect();
    let at = head.rfind(char::is_whitespace).unwrap_or(head.len());
    format!("{} ...", head[..at].trim_end())
}

impl Notes {
    /// The notes of a project, from each note file's id and text, with the files that are not
    /// notes (id, why): a file a person left half written does not hide the rest. At most
    /// [`MAX_NOTES`] are read, in id order.
    pub fn read<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> (Notes, Vec<(String, NoteError)>) {
        let mut files: Vec<(&str, &str)> = files.into_iter().collect();
        files.sort_by_key(|(id, _)| *id);
        let (mut notes, mut bad) = (Vec::new(), Vec::new());
        for (id, text) in files.into_iter().take(MAX_NOTES) {
            match Note::parse(id, text) {
                Ok(note) => notes.push(note),
                Err(why) => bad.push((id.to_owned(), why)),
            }
        }
        (Notes { notes }, bad)
    }

    /// Every note, current or not, in id order.
    pub fn all(&self) -> &[Note] {
        &self.notes
    }

    /// The note with this id, current or not.
    pub fn exact(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    /// The note that replaced `id`, if one did.
    fn replaced_by(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.supersedes.as_deref() == Some(id))
    }

    /// Whether another note replaces this one.
    pub fn superseded(&self, id: &str) -> bool {
        self.replaced_by(id).is_some()
    }

    /// The note `id` names now: itself, or the newest in the chain of notes that replaced it.
    pub fn get(&self, id: &str) -> Option<&Note> {
        let mut note = self.exact(id)?;
        // (A chain is at most as long as there are notes: two that replace each other end it.)
        for _ in 0..self.notes.len() {
            match self.replaced_by(&note.id) {
                Some(newer) => note = newer,
                None => break,
            }
        }
        Some(note)
    }

    /// The notes nothing has replaced, in id order.
    pub fn current(&self) -> impl Iterator<Item = &Note> {
        self.notes.iter().filter(|n| !self.superseded(&n.id))
    }

    /// Current notes of `kind` (any when `None`) that hold every one of `words` in their title,
    /// their text or their id, the ones with more of them in the title first. No words: all.
    pub fn list(&self, kind: Option<Kind>, words: &str) -> Vec<&Note> {
        let words: Vec<String> = words.split_whitespace().map(str::to_lowercase).collect();
        let mut found: Vec<(usize, &Note)> = self
            .current()
            .filter(|n| kind.is_none_or(|k| n.kind == k))
            .filter_map(|n| {
                let (title, body) = (format!("{} {}", n.title.to_lowercase(), n.id), n.body.to_lowercase());
                let all = words.iter().all(|w| title.contains(w.as_str()) || body.contains(w.as_str()));
                all.then(|| (words.iter().filter(|w| title.contains(w.as_str())).count(), n))
            })
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.kind.cmp(&b.1.kind)).then(a.1.id.cmp(&b.1.id)));
        found.into_iter().map(|(_, n)| n).collect()
    }

    /// Current notes about `path`, the ones that name it exactly first.
    pub fn about(&self, path: &str) -> Vec<&Note> {
        let mut found: Vec<&Note> = self.current().filter(|n| n.is_about(path)).collect();
        found.sort_by_key(|n| (!n.about.iter().any(|a| a == path), n.kind, n.id.clone()));
        found
    }

    /// What an agent is given at its first call: what the game is, how it is built and checked,
    /// and the titles of everything else the project remembers, in at most [`MAX_BRIEF`]
    /// characters. Empty when the project remembers nothing.
    pub fn brief(&self) -> String {
        let first = |kind: Kind| self.current().find(|n| n.kind == kind);
        let mut out = String::new();
        if let Some(brief) = first(Kind::Brief) {
            out.push_str(&format!("{}\n{}\n", brief.title, cut(&brief.body, 800)));
        }
        if let Some(how) = first(Kind::How) {
            out.push_str(&format!("\nHow it is built and checked ({}):\n{}\n", how.id, cut(&how.body, 1000)));
        }
        let rest: Vec<&Note> = self.current().filter(|n| Some(*n) != first(Kind::Brief) && Some(*n) != first(Kind::How)).collect();
        if !rest.is_empty() {
            out.push_str("\nAlso remembered (memory_get ID reads one):\n");
            let mut shown = 0;
            for kind in Kind::ALL {
                for note in rest.iter().filter(|n| n.kind == kind) {
                    let line = format!("- {} [{}] {}\n", note.id, kind.name(), note.title);
                    if out.chars().count() + line.chars().count() + 40 > MAX_BRIEF {
                        break;
                    }
                    out.push_str(&line);
                    shown += 1;
                }
            }
            if shown < rest.len() {
                out.push_str(&format!("- and {} more (memory_list)\n", rest.len() - shown));
            }
        }
        cut(out.trim_end(), MAX_BRIEF)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DECISION: &str = "---\ntitle: The gate has ten lives on every difficulty\nkind: decision\nsupersedes: gate-lives\nabout: modules/gate/src/lib.rs, game.json\n---\nDifficulty changes how fast runners come, never the gate's lives.\n";

    fn note(id: &str, kind: &str, title: &str, extra: &str, body: &str) -> (String, String) {
        (id.to_owned(), format!("---\ntitle: {title}\nkind: {kind}\n{extra}---\n{body}\n"))
    }

    fn notes(files: &[(String, String)]) -> Notes {
        let (notes, bad) = Notes::read(files.iter().map(|(id, text)| (id.as_str(), text.as_str())));
        assert!(bad.is_empty(), "{bad:?}");
        notes
    }

    #[test]
    fn a_note_is_read_and_written_back() {
        let n = Note::parse("gate-lives-2", DECISION).unwrap();
        assert_eq!((n.title.as_str(), n.kind, n.supersedes.as_deref()), ("The gate has ten lives on every difficulty", Kind::Decision, Some("gate-lives")));
        assert_eq!(n.about, ["modules/gate/src/lib.rs", "game.json"]);
        assert_eq!(n.body, "Difficulty changes how fast runners come, never the gate's lives.");
        assert_eq!(n.to_text(), DECISION);
        assert_eq!(n.path(), "notes/gate-lives-2.md");
        // Windows line ends, a byte order mark and no text at all are read too.
        assert_eq!(Note::parse("gate-lives-2", &format!("\u{feff}{}", DECISION.replace('\n', "\r\n"))).unwrap(), n);
        let bare = Note::parse("a", "---\ntitle: A fact\nkind: fact\n---").unwrap();
        assert_eq!((bare.body.as_str(), bare.to_text().as_str()), ("", "---\ntitle: A fact\nkind: fact\n---\n"));
    }

    #[test]
    fn a_file_that_is_not_a_note_says_what_a_note_is() {
        let code = |id: &str, text: &str| Note::parse(id, text).unwrap_err().code;
        assert_eq!(code("a", "just some text"), "no_front_matter");
        assert_eq!(code("a", "---\ntitle: x\nkind: fact\n"), "no_front_matter");
        assert_eq!(code("a", "---\ntitle: x\n---\n"), "bad_front_matter");
        assert_eq!(code("a", "---\nkind: fact\n---\n"), "bad_front_matter");
        assert_eq!(code("a", "---\ntitle: x\nkind: idea\n---\n"), "bad_kind");
        assert_eq!(code("a", "---\ntitle: x\nkind: fact\nauthor: me\n---\n"), "bad_front_matter");
        assert_eq!(code("a", "---\ntitle: x\nkind: fact\nabout: ../outside\n---\n"), "bad_about");
        assert_eq!(code("a", "---\ntitle: x\nkind: fact\nabout: C:/x\n---\n"), "bad_about");
        assert_eq!(code("a", "---\ntitle: x\nkind: fact\nsupersedes: a\n---\n"), "bad_supersedes");
        assert_eq!(code("Bad Id", "---\ntitle: x\nkind: fact\n---\n"), "bad_id");
        assert_eq!(code("a", &format!("---\ntitle: {}\nkind: fact\n---\n", "x".repeat(MAX_TITLE + 1))), "bad_title");
        assert_eq!(code("a", &format!("---\ntitle: x\nkind: fact\n---\n{}", "x".repeat(MAX_NOTE_BYTES))), "too_big");
    }

    #[test]
    fn an_id_is_made_from_a_title() {
        assert_eq!(id_from("The gate has ten lives!"), "the-gate-has-ten-lives");
        assert_eq!(id_from("  Wave 5 -- the boss  "), "wave-5-the-boss");
        assert_eq!(id_from("???"), "note");
        assert!(id_ok(&id_from(&"long word ".repeat(20))) && id_from(&"long word ".repeat(20)).len() <= 48);
    }

    #[test]
    fn a_note_that_was_replaced_is_kept_and_not_listed() {
        let all = notes(&[
            note("gate-lives", "decision", "The gate has five lives", "", "Five."),
            note("gate-lives-2", "decision", "The gate has ten lives", "supersedes: gate-lives\n", "Ten."),
            note("gate-lives-3", "decision", "The gate has ten lives on every difficulty", "supersedes: gate-lives-2\n", "Ten, always."),
            note("waves", "fact", "There are five waves", "", "Five waves."),
        ]);
        assert_eq!(all.current().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["gate-lives-3", "waves"]);
        assert!(all.superseded("gate-lives") && all.superseded("gate-lives-2") && !all.superseded("gate-lives-3"));
        // Asking for an old one answers with what replaced it; the old text is still there.
        assert_eq!(all.get("gate-lives").map(|n| n.id.as_str()), Some("gate-lives-3"));
        assert_eq!(all.exact("gate-lives").map(|n| n.body.as_str()), Some("Five."));
        assert_eq!(all.get("nothing"), None);
        // Two notes that replace each other do not hang it.
        let ring = notes(&[note("a", "fact", "A", "supersedes: b\n", ""), note("b", "fact", "B", "supersedes: a\n", "")]);
        assert!(ring.get("a").is_some());
    }

    #[test]
    fn notes_are_found_by_kind_by_words_and_by_what_they_are_about() {
        let all = notes(&[
            note("brief", "brief", "Hold the gate: a wave defence", "", "Runners come in waves; the player holds a gate."),
            note("how", "how", "How it is built and checked", "", "One engine left running; logic_build with reload."),
            note("gate-lives", "decision", "The gate has ten lives", "about: modules/gate/src/lib.rs, game.json\n", "Difficulty changes speed only."),
            note("runner-kinds", "fact", "Runner kinds", "about: modules/gate/\n", "A fast one and a slow one. The gate takes one life from each."),
            note("open-sounds", "thread", "No sound for a lost life yet", "", "Needs a recipe."),
        ]);
        assert_eq!(all.list(Some(Kind::Decision), "").iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["gate-lives"]);
        assert_eq!(all.list(None, "").len(), 5);
        // Every word must be there; a word in the title comes first.
        assert_eq!(all.list(None, "gate lives").iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["gate-lives"]);
        assert_eq!(all.list(None, "GATE").iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["brief", "gate-lives", "runner-kinds"]);
        assert!(all.list(None, "dragon").is_empty());
        // About a file: the note that names it, then the one about its folder.
        assert_eq!(all.about("modules/gate/src/lib.rs").iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["gate-lives", "runner-kinds"]);
        assert_eq!(all.about("game.json").iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["gate-lives"]);
        assert!(all.about("modules/gatehouse/x.rs").is_empty(), "a folder is not a prefix of another folder's name");
        assert!(all.about("assets/ui/hud.json").is_empty());
    }

    #[test]
    fn the_brief_is_what_the_game_is_how_it_is_built_and_the_rest_by_title() {
        let all = notes(&[
            note("brief", "brief", "Hold the gate: a wave defence", "", "Runners come in waves; the player holds a gate."),
            note("how", "how", "How it is built and checked", "", "One engine left running; logic_build with reload."),
            note("gate-lives", "decision", "The gate has ten lives", "", "Difficulty changes speed only."),
            note("old", "fact", "An old fact", "", "x"),
            note("new", "fact", "A newer fact", "supersedes: old\n", "y"),
        ]);
        let brief = all.brief();
        assert!(brief.starts_with("Hold the gate: a wave defence\nRunners come in waves"), "{brief}");
        assert!(brief.contains("How it is built and checked (how):\nOne engine left running"), "{brief}");
        assert!(brief.contains("- gate-lives [decision] The gate has ten lives") && brief.contains("- new [fact] A newer fact"), "{brief}");
        assert!(!brief.contains("An old fact"), "a replaced note is not in it");
        assert_eq!(Notes::default().brief(), "", "a project that remembers nothing says nothing");
    }

    #[test]
    fn the_brief_is_bounded_whatever_the_project_holds() {
        let long = "word ".repeat(2000);
        let mut files = vec![note("brief", "brief", "A game", "", &long), note("how", "how", "How", "", &long)];
        files.extend((0..400).map(|i| note(&format!("fact-{i:03}"), "fact", &format!("Fact number {i} with a title of some length to fill the list"), "", "x")));
        let brief = notes(&files).brief();
        assert!(brief.chars().count() <= MAX_BRIEF, "{}", brief.chars().count());
        assert!(brief.contains(" ...") && brief.contains("more (memory_list)"), "what was cut is said");
    }

    #[test]
    fn a_half_written_file_does_not_hide_the_rest() {
        let files = [note("good", "fact", "A fact", "", "x"), ("half".to_owned(), "---\ntitle: unfinished".to_owned())];
        let (all, bad) = Notes::read(files.iter().map(|(id, text)| (id.as_str(), text.as_str())));
        assert_eq!(all.all().len(), 1);
        assert_eq!(bad.iter().map(|(id, e)| (id.as_str(), e.code)).collect::<Vec<_>>(), [("half", "no_front_matter")]);
    }
}

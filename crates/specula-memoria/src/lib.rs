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
/// may end in `/`. No comma: the front matter's `about` line separates paths with one.
fn path_ok(path: &str) -> bool {
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    !trimmed.is_empty() && path.len() <= 200 && trimmed.split('/').all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains(['\\', ':', ',']) && !part.chars().any(char::is_control))
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
    /// Checks a note's parts and its size: what a note that is about to be written is held to.
    pub fn check(&self) -> Result<(), NoteError> {
        self.check_fields()?;
        if self.to_text().len() > MAX_NOTE_BYTES {
            return Err(NoteError::new("too_big", format!("a note is at most {} KB: a longer one is two notes", MAX_NOTE_BYTES / 1024)));
        }
        Ok(())
    }

    /// Checks a note's parts (its id, title, what it replaces, what it is about): everything
    /// its front matter says. Its size is not a part: a file's is its own length
    /// ([`Note::parse`]), a new note's that of its text ([`Note::check`]).
    fn check_fields(&self) -> Result<(), NoteError> {
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
        Ok(())
    }

    /// Reads a note from its file's text. `id`: the file's name without `.md`.
    ///
    /// A text is a note when it is no longer than a note may be, starts with a front matter
    /// ([`split`]), and that front matter is one ([`Note::of_front`]). Nothing else decides
    /// it, so that whoever reads only the first lines ([`front`]) decides the same.
    pub fn parse(id: &str, text: &str) -> Result<Note, NoteError> {
        if text.len() > MAX_NOTE_BYTES {
            return Err(NoteError::new("too_big", format!("a note is at most {} KB", MAX_NOTE_BYTES / 1024)));
        }
        let text = text.replace("\r\n", "\n");
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let (front, body) = split(text).ok_or_else(no_front)?;
        let mut note = Note::of_front(id, front)?;
        note.body = body.trim_matches('\n').to_owned();
        Ok(note)
    }

    /// The note a front matter describes, with no text yet: its lines read and its parts
    /// checked. One reading, for a note's whole text and for its first lines alone.
    fn of_front(id: &str, front: &str) -> Result<Note, NoteError> {
        if front.lines().count() > FRONT_LINES {
            return Err(NoteError::new("bad_front_matter", format!("a note's front matter is at most {FRONT_LINES} lines (it has four keys: title, kind, supersedes, about)")));
        }
        let mut note = Note { id: id.to_owned(), title: String::new(), kind: Kind::Fact, supersedes: None, about: Vec::new(), body: String::new() };
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
        note.check_fields()?;
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
///
/// **Read to hold together, whatever the files say.** A note names the note it replaces, and
/// a replaced note drops out of every list. Files are written by many hands and many ways (a
/// tool, an undo, an editor, a pull), so the files alone may say what cannot be: two notes
/// that replace each other, directly or through others. Taken as written, every note of such
/// a ring would be replaced and none would be listed. So a replacement that would close a ring
/// is not followed ([`Notes::read`], in id order: the note keeps its line, and
/// [`Notes::problems`] says so), and no way of writing can hide notes from their readers
/// (review R2 of M6 part 3: a check at the write was got past three ways).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notes {
    notes: Vec<Note>,
    /// The replacements that are followed: the newer note's id, and the id it replaces.
    follows: std::collections::BTreeMap<String, String>,
    /// The ids some note replaces (made once when the notes are read: whether a note is
    /// current is asked for every note of every listing).
    replaced: std::collections::BTreeSet<String>,
    /// The replacements not followed, because they would close a ring: (the note, what its
    /// file says it replaces).
    rings: Vec<(String, String)>,
    /// Notes past the [`MAX_NOTES`] that are read.
    unread: usize,
    /// Files of the notes folder that are not notes.
    not_notes: usize,
}

/// Most files that are not notes that a reading names (the rest are counted).
pub const MAX_NOT_NOTES: usize = 64;

/// The replacements that are followed among notes given in id order as (id, what its file
/// says it replaces), and the ones that are not because they would close a ring: each note's
/// line is followed unless the note it names already leads back to it through the lines
/// followed so far. One rule, for the reader and for whoever checks a write against it.
pub fn follow<'a>(notes: impl Iterator<Item = (&'a str, Option<&'a str>)>) -> (std::collections::BTreeMap<String, String>, Vec<(String, String)>) {
    let notes: Vec<(&str, Option<&str>)> = notes.collect();
    let mut follows: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut rings = Vec::new();
    for (id, old) in &notes {
        let Some(old) = old else { continue };
        let mut at = Some(*old);
        let mut ring = false;
        // (A chain is at most as long as there are notes.)
        for _ in 0..=notes.len() {
            match at {
                Some(a) if a == *id => {
                    ring = true;
                    break;
                }
                Some(a) => at = follows.get(a).map(String::as_str),
                None => break,
            }
        }
        if ring {
            rings.push(((*id).to_owned(), (*old).to_owned()));
        } else {
            follows.insert((*id).to_owned(), (*old).to_owned());
        }
    }
    (follows, rings)
}

/// The first `max` characters of `text`, cut at a word and marked when something was cut.
pub fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max.saturating_sub(4)).collect();
    let at = head.rfind(char::is_whitespace).unwrap_or(head.len());
    format!("{} ...", head[..at].trim_end())
}

impl Notes {
    /// The notes of a project, from each note file's id and text, with the files that are not
    /// notes (id, why): a file a person left half written does not hide the rest.
    ///
    /// **Only notes count.** The first [`MAX_NOTES`] notes by id are read; a file that is not
    /// a note takes no note's place (review R3 of M6 part 3: the first 1,024 *files* were
    /// read, so that many drafts with ids that sort first left a project with no notes for
    /// anyone, and any agent can write a draft). Notes past the limit are counted
    /// ([`Notes::unread`]), and at most [`MAX_NOT_NOTES`] files that are not notes are named,
    /// the rest counted ([`Notes::not_notes`]).
    pub fn read<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> (Notes, Vec<(String, NoteError)>) {
        let mut files: Vec<(&str, &str)> = files.into_iter().collect();
        files.sort_by_key(|(id, _)| *id);
        let (mut notes, mut bad) = (Vec::new(), Vec::new());
        let (mut unread, mut not_notes) = (0, 0);
        for (id, text) in files {
            match Note::parse(id, text) {
                Ok(note) if notes.len() < MAX_NOTES => notes.push(note),
                Ok(_) => unread += 1,
                Err(why) => {
                    not_notes += 1;
                    if bad.len() < MAX_NOT_NOTES {
                        bad.push((id.to_owned(), why));
                    }
                }
            }
        }
        let (follows, rings) = follow(notes.iter().map(|n: &Note| (n.id.as_str(), n.supersedes.as_deref())));
        let replaced = follows.values().cloned().collect();
        (Notes { notes, follows, replaced, rings, unread, not_notes }, bad)
    }

    /// Notes the project holds past the [`MAX_NOTES`] that are read (by id).
    pub fn unread(&self) -> usize {
        self.unread
    }

    /// How many files of the notes folder are not notes (every one, named or not).
    pub fn not_notes(&self) -> usize {
        self.not_notes
    }

    /// What the files say that the notes are not read as, in words for whoever keeps them: a
    /// replacement that would close a ring (not followed), a second current brief (the first
    /// by id is the brief), and notes past the limit. Empty for notes written through the tools.
    pub fn problems(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .rings
            .iter()
            .map(|(id, old)| {
                format!("{id} says it supersedes {old}, and {old} already replaces {id}, directly or through other notes (a ring): that line of {id} is not followed (the other lines of the ring are); take it out of {FOLDER}/{id}.md")
            })
            .collect();
        let mut briefs = self.current().filter(|n| n.kind == Kind::Brief);
        if let (Some(first), Some(second)) = (briefs.next(), briefs.next()) {
            out.push(format!("there are two briefs, {} and {}: {} is given as the brief; make the other a fact, or have one supersede the other", first.id, second.id, first.id));
        }
        if self.unread > 0 {
            out.push(format!("the project holds {} more notes than the {MAX_NOTES} that are read (the first by id): delete notes that are no longer needed (project_delete)", self.unread));
        }
        out
    }

    /// Every note, current or not, in id order.
    pub fn all(&self) -> &[Note] {
        &self.notes
    }

    /// The note with this id, current or not.
    pub fn exact(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    /// The note that replaced `id`, if one did (the first by id, when the files name two).
    fn replaced_by(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| self.follows.get(&n.id).is_some_and(|old| old == id))
    }

    /// The id of the note that replaced `id`, if one did.
    pub fn superseded_by(&self, id: &str) -> Option<&str> {
        self.replaced_by(id).map(|n| n.id.as_str())
    }

    /// Whether another note replaces this one.
    pub fn superseded(&self, id: &str) -> bool {
        self.replaced.contains(id)
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

    /// What an agent is given at its first call, in parts: the brief (what the game is), the
    /// first `how` note (how it is built and checked), and every other current note, by kind
    /// and then by id. Whoever shows them says whose words they are: a project's notes are
    /// written by the agents and people who worked on it, never by the engine.
    pub fn brief_parts(&self) -> Brief<'_> {
        let first = |kind: Kind| self.current().find(|n| n.kind == kind);
        let (brief, how) = (first(Kind::Brief), first(Kind::How));
        let is = |part: Option<&Note>, n: &Note| part.is_some_and(|p| p.id == n.id);
        let mut rest: Vec<&Note> = self.current().filter(|n| !is(brief, n) && !is(how, n)).collect();
        rest.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.id.cmp(&b.id)));
        Brief { brief, how, rest }
    }

    /// [`Notes::brief_parts`] as one text of at most [`MAX_BRIEF`] characters: the brief's
    /// title and text, the how-to's text, and the titles of the rest as far as they fit.
    /// Empty when the project remembers nothing.
    pub fn brief(&self) -> String {
        let parts = self.brief_parts();
        let mut out = String::new();
        if let Some(brief) = parts.brief {
            out.push_str(&format!("{}\n{}\n", brief.title, cut(&brief.body, BRIEF_TEXT)));
        }
        if let Some(how) = parts.how {
            out.push_str(&format!("\nHow it is built and checked ({}):\n{}\n", how.id, cut(&how.body, HOW_TEXT)));
        }
        if !parts.rest.is_empty() {
            out.push_str("\nAlso remembered (memory_get ID reads one):\n");
            let mut shown = 0;
            let mut used = out.chars().count();
            for note in &parts.rest {
                let line = format!("- {} [{}] {}\n", note.id, note.kind.name(), note.title);
                let len = line.chars().count();
                if used + len + 40 > MAX_BRIEF {
                    break;
                }
                out.push_str(&line);
                used += len;
                shown += 1;
            }
            if shown < parts.rest.len() {
                out.push_str(&format!("- and {} more (memory_list)\n", parts.rest.len() - shown));
            }
        }
        cut(out.trim_end(), MAX_BRIEF)
    }
}

/// Most characters of the brief's text, and of the how-to's, given at a first call.
pub const BRIEF_TEXT: usize = 800;
pub const HOW_TEXT: usize = 1000;

/// [`Notes::brief_parts`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Brief<'n> {
    pub brief: Option<&'n Note>,
    pub how: Option<&'n Note>,
    /// Every other current note, by kind and then by id.
    pub rest: Vec<&'n Note>,
}

/// Most lines of a note's front matter, blank ones counted (it has four keys). A bound, so
/// that whoever looks at every note of a project at once ([`front`]) reads a known few lines
/// of each; [`Note::parse`] holds a note to it, so the two never disagree about a note.
pub const FRONT_LINES: usize = 16;

/// A note's text (line ends `\n`, no byte-order mark) as its front matter and its body.
fn split(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    match rest.split_once("\n---\n") {
        Some(parts) => Some(parts),
        None => Some((rest.strip_suffix("\n---")?, "")),
    }
}

fn no_front() -> NoteError {
    NoteError::new("no_front_matter", "a note starts with front matter between two lines of ---: title, kind, and optionally supersedes and about")
}

/// What a note says of its place among the others: its kind, and the note it replaces. Read
/// from the first lines alone, for whoever must look at every note of a project at once (the
/// check made where a note is committed). `id`: the file's name without `.md`.
///
/// `Some` exactly for the texts [`Note::parse`] takes, with what the parsed note says: the
/// same size rule, the same way of splitting ([`split`]) and the same reading of the front
/// matter ([`Note::of_front`]). A file in the notes folder that is not a note (a draft, a
/// front matter with no title) is `None` here as it is no note to the reader (review R4 of M6
/// part 3: it answered for such drafts, a thousand of them filled the check's view, and the
/// check then accepted what the reader reported).
pub fn front(id: &str, text: &str) -> Option<(Kind, Option<String>)> {
    if text.len() > MAX_NOTE_BYTES {
        return None;
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // The lines a front matter can be in: its opening line, its lines, its closing line.
    let mut head = text;
    let mut lines = 0;
    for (at, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            lines += 1;
            if lines == FRONT_LINES + 2 {
                head = &text[..=at];
                break;
            }
        }
    }
    let head = head.replace("\r\n", "\n");
    let (front, _) = split(&head)?;
    let note = Note::of_front(id, front).ok()?;
    Some((note.kind, note.supersedes))
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

    /// Review R1 of M6 part 3: a path with a comma came back as two paths (the `about` line
    /// separates paths with one), so it is not a path a note may be about.
    #[test]
    fn a_path_with_a_comma_is_not_one_a_note_is_about() {
        let n = Note { id: "a".into(), title: "A fact".into(), kind: Kind::Fact, supersedes: None, about: vec!["assets/x,y.json".into()], body: String::new() };
        assert_eq!(n.check().unwrap_err().code, "bad_about");
        let plain = Note { about: vec!["assets/x y.json".into(), "modules/gate/".into()], ..n };
        assert_eq!(Note::parse("a", &plain.to_text()).unwrap().about, plain.about, "what is written is what is read back");
    }

    /// The first lines alone say a note's kind and what it replaces: enough for whoever holds
    /// every note of a project against one being written.
    #[test]
    fn a_notes_place_is_read_from_its_first_lines() {
        assert_eq!(front("x", DECISION), Some((Kind::Decision, Some("gate-lives".to_owned()))));
        assert_eq!(front("x", "---\ntitle: x\nkind: brief\n---\ntext"), Some((Kind::Brief, None)));
        assert_eq!(front("x", "\u{feff}---\r\ntitle: x\r\nkind: how\r\n---\r\n"), Some((Kind::How, None)));
        assert_eq!(front("x", "just text"), None);
        assert_eq!(front("x", "---\ntitle: x\n---\n"), None, "no kind");
        assert_eq!(front("x", "---\ntitle: x\nkind: fact\n"), None, "the front matter never ends");
        assert_eq!(front("x", &format!("---\nkind: fact\n{}---\n", "x: y\n".repeat(40))), None, "too long to be a front matter");
        // What is no note to the reader has no place here either: no title, an id that is not
        // one, a note that names itself, a text longer than a note may be.
        assert_eq!(front("x", "---\nkind: fact\n---\n"), None, "no title");
        assert_eq!(front("x", "---\ntitle: x\nkind: fact\nsupersedes: Not An Id\n---\n"), None);
        assert_eq!(front("x", "---\ntitle: x\nkind: fact\nsupersedes: x\n---\n"), None);
        assert_eq!(front("Not An Id", "---\ntitle: x\nkind: fact\n---\n"), None);
        assert_eq!(front("x", &format!("---\ntitle: x\nkind: fact\n---\n{}", "y".repeat(MAX_NOTE_BYTES))), None);
    }

    /// The brief is made in one pass however many notes there are, and stays within its size
    /// (review R1 of M6 part 3: it asked "is this note current" by scanning every note for
    /// every note for every note, 1.1 s at a project's full 1,024 notes, at every first call).
    /// The measure is what it asks, not a time: each note is looked up in a set.
    #[test]
    fn the_brief_of_a_full_memory_is_one_pass_and_within_its_size() {
        let mut files: Vec<(String, String)> = (0..MAX_NOTES - 2).map(|i| note(&format!("n{i:04}"), "fact", &format!("Fact number {i} of the game, said as one sentence"), "", "x")).collect();
        files.push(note("brief", "brief", "The game", "", "What it is."));
        files.push(note("how", "how", "How it is built", "", "The loop."));
        // Half of the facts replace another: those are not current.
        for i in (1..MAX_NOTES - 2).step_by(2) {
            files[i] = note(&format!("n{i:04}"), "fact", &format!("Fact {i}, again"), &format!("supersedes: n{:04}\n", i - 1), "x");
        }
        let all = notes(&files);
        assert_eq!(all.replaced.len(), (MAX_NOTES - 2) / 2);
        let parts = all.brief_parts();
        assert_eq!((parts.brief.map(|n| n.id.as_str()), parts.how.map(|n| n.id.as_str())), (Some("brief"), Some("how")));
        assert_eq!(parts.rest.len(), all.current().count() - 2);
        assert!(parts.rest.iter().all(|n| !all.superseded(&n.id)));
        let text = all.brief();
        assert!(text.chars().count() <= MAX_BRIEF && text.contains("more (memory_list)"), "{}", text.chars().count());
    }

    #[test]
    fn an_id_is_made_from_a_title() {
        assert_eq!(id_from("The gate has ten lives!"), "the-gate-has-ten-lives");
        assert_eq!(id_from("  Wave 5 -- the boss  "), "wave-5-the-boss");
        assert_eq!(id_from("???"), "note");
        assert!(id_ok(&id_from(&"long word ".repeat(20))) && id_from(&"long word ".repeat(20)).len() <= 48);
    }

    /// Whatever the files say, no note is hidden by a ring (review R2 of M6 part 3: a ring
    /// written past the check at the write, by an undo, an edit on disk or a padded front
    /// matter, took both its notes out of every list).
    #[test]
    fn a_ring_in_the_files_hides_no_note() {
        let ring = [note("p", "fact", "P", "supersedes: q\n", "p"), note("q", "fact", "Q", "supersedes: p\n", "q"), note("z", "fact", "Z", "", "z")];
        let (all, bad) = Notes::read(ring.iter().map(|(id, text)| (id.as_str(), text.as_str())));
        assert!(bad.is_empty());
        // In id order: p replaces q; q's line would close the ring and is not followed.
        assert_eq!(all.current().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["p", "z"]);
        assert_eq!((all.superseded_by("q"), all.superseded_by("p")), (Some("p"), None));
        assert_eq!(all.get("q").map(|n| n.id.as_str()), Some("p"));
        let problems = all.problems();
        assert!(problems.len() == 1 && problems[0].contains("q says it supersedes p") && problems[0].contains("notes/q.md"), "{problems:?}");
        // A ring of three: one line is not followed. (A note that names itself is refused.)
        let three = [note("a", "fact", "A", "supersedes: c\n", ""), note("b", "fact", "B", "supersedes: a\n", ""), note("c", "fact", "C", "supersedes: b\n", ""), note("s", "fact", "S", "supersedes: s\n", "")];
        let (all, bad) = Notes::read(three.iter().map(|(id, text)| (id.as_str(), text.as_str())));
        assert_eq!(bad.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["s"], "a note that names itself is not a note");
        assert_eq!(all.current().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["b"], "a replaced c, b replaced a; the line of c is not followed");
        assert_eq!(all.problems().len(), 1);
        // Notes with no ring have no problems, and two notes may name one older note: it is
        // replaced, and both of them stay listed.
        let two = [note("old", "fact", "Old", "", ""), note("n1", "fact", "N1", "supersedes: old\n", ""), note("n2", "fact", "N2", "supersedes: old\n", "")];
        let all = notes(&two);
        assert_eq!(all.current().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n1", "n2"]);
        assert!(all.problems().is_empty());
        // Two briefs: both listed, the first by id given, and it is said.
        let briefs = [note("brief", "brief", "The game", "", ""), note("pitch", "brief", "Another", "", "")];
        let all = notes(&briefs);
        assert_eq!(all.brief_parts().brief.map(|n| n.id.as_str()), Some("brief"));
        assert!(all.problems().iter().any(|p| p.contains("two briefs, brief and pitch")));
    }

    /// Files that are not notes take no note's place, however many there are (review R3 of M6
    /// part 3: 1,030 drafts with ids that sort first left a project with no notes for anyone).
    #[test]
    fn drafts_never_crowd_the_notes_out() {
        let mut files: Vec<(String, String)> = (0..MAX_NOTES + 6).map(|i| (format!("{i:04}"), "draft".to_owned())).collect();
        files.push(note("brief", "brief", "The game", "", "what it is"));
        files.push(note("how", "how", "How it is built", "", "logic_build"));
        let (all, bad) = Notes::read(files.iter().map(|(id, text)| (id.as_str(), text.as_str())));
        assert_eq!(all.current().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["brief", "how"]);
        assert_eq!(all.brief_parts().brief.map(|n| n.id.as_str()), Some("brief"));
        assert_eq!((all.not_notes(), bad.len(), all.unread()), (MAX_NOTES + 6, MAX_NOT_NOTES, 0));
        assert!(all.problems().is_empty());
        // More notes than are read: the first by id are, and it is said how many are not.
        let many: Vec<(String, String)> = (0..MAX_NOTES + 5).map(|i| note(&format!("n{i:04}"), "fact", &format!("Note {i}"), "", "")).collect();
        let (all, bad) = Notes::read(many.iter().map(|(id, text)| (id.as_str(), text.as_str())));
        assert!(bad.is_empty());
        assert_eq!((all.all().len(), all.unread()), (MAX_NOTES, 5));
        assert!(all.problems().iter().any(|p| p.contains("5 more notes than the 1024 that are read")), "{:?}", all.problems());
    }

    /// One rule says which replacements are followed, for the reader and for a check of a
    /// write: adding a line that closes a ring adds a line that is not followed.
    #[test]
    fn a_write_that_closes_a_ring_adds_a_line_that_is_not_followed() {
        let rings = |edges: &[(&str, Option<&str>)]| follow(edges.iter().copied()).1;
        assert!(rings(&[("p", Some("q")), ("q", None)]).is_empty());
        assert_eq!(rings(&[("p", Some("q")), ("q", Some("p"))]), [("q".to_owned(), "p".to_owned())]);
        // Rewriting p (which keeps its line) in a ring that is already there adds none: the
        // same line is the one not followed before and after.
        assert_eq!(rings(&[("p", Some("q")), ("q", Some("p"))]), rings(&[("p", Some("q")), ("q", Some("p"))]));
        // Taking the line out of either ends it.
        assert!(rings(&[("p", None), ("q", Some("p"))]).is_empty());
        assert!(rings(&[("p", Some("q")), ("q", None)]).is_empty());
        // A note that names itself, and one that names a note that is not there.
        assert_eq!(rings(&[("s", Some("s"))]), [("s".to_owned(), "s".to_owned())]);
        assert!(rings(&[("t", Some("gone"))]).is_empty());
    }

    /// The first-lines reader and the parser agree about every text that is a note, and a
    /// front matter padded past its bound is not a note for either (review R2: twenty blank
    /// lines before `supersedes:` were a note to the parser and nothing to the first-lines
    /// reader, so the check at the write let a ring through).
    #[test]
    fn the_first_lines_say_what_the_parsed_note_says() {
        let blank = "\n".repeat(20);
        let padded = format!("---\ntitle: T\nkind: fact\n{blank}supersedes: other\n---\ntext\n");
        assert_eq!(Note::parse("x", &padded).unwrap_err().code, "bad_front_matter");
        assert_eq!(front("x", &padded), None);
        let fits = format!("---\ntitle: T\n{}kind: brief\nsupersedes: other\n---\ntext\n", "\n".repeat(FRONT_LINES - 3));
        let one_more = format!("---\ntitle: T\n{}kind: brief\nsupersedes: other\n---\ntext\n", "\n".repeat(FRONT_LINES - 2));
        assert!(Note::parse("x", &fits).is_ok() && Note::parse("x", &one_more).is_err());
        let texts = [
            DECISION.to_owned(),
            fits,
            one_more,
            "---\ntitle: T\nkind: how\n---".to_owned(),
            "---\r\ntitle: T\r\nkind: how\r\nsupersedes: a-b\r\n---\r\nbody\r\n".to_owned(),
            "\u{feff}---\ntitle: T\nkind: brief\n---\n".to_owned(),
            "---\ntitle: T\nkind: fact\nkind: brief\nsupersedes: a\nsupersedes: b\n---\n".to_owned(),
            "---\n\n\ntitle: T\n\nkind: decision\n\n---\n---\nsupersedes: not-front\n---\n".to_owned(),
            "--- \ntitle: T\nkind: fact\n---\n".to_owned(),
            "---\ntitle: T\nkind: fact\n--- \nsupersedes: a\n---\n".to_owned(),
            "no front matter".to_owned(),
            String::new(),
            // Not notes, each for one reason: no title, a key that is not one, a line that is
            // not `key: value`, a kind that is not one, a title too long, an `about` that is
            // no path, what it replaces is itself, and a text past a note's size.
            "---\nkind: brief\n---\n".to_owned(),
            "---\ntitle: T\nkind: fact\nauthor: me\n---\n".to_owned(),
            "---\ntitle: T\nkind: fact\njust words\n---\n".to_owned(),
            "---\ntitle: T\nkind: opinion\n---\n".to_owned(),
            format!("---\ntitle: {}\nkind: fact\n---\n", "T".repeat(MAX_TITLE + 1)),
            "---\ntitle: T\nkind: fact\nabout: ../outside\n---\n".to_owned(),
            "---\ntitle: T\nkind: fact\nsupersedes: x\n---\n".to_owned(),
            format!("---\ntitle: T\nkind: fact\n---\n{}", "y".repeat(MAX_NOTE_BYTES)),
            // A note at exactly a note's size, and a note whose line ends are all `\r\n`.
            {
                let head = "---\ntitle: T\nkind: fact\n---\n";
                format!("{head}{}", "y".repeat(MAX_NOTE_BYTES - head.len()))
            },
            format!("---\r\ntitle: T\r\nkind: fact\r\n---\r\n{}", "line\r\n".repeat(40)),
        ];
        // The two say the same of every text: a note to one is a note to the other, with the
        // same kind and the same note replaced; what is not a note is one to neither (review
        // R4: the first-lines reader answered for drafts the parser refused).
        let mut notes = 0;
        for text in &texts {
            let parsed = Note::parse("x", text).ok();
            notes += usize::from(parsed.is_some());
            assert_eq!(front("x", text), parsed.map(|note| (note.kind, note.supersedes)), "{:?}", text.chars().take(80).collect::<String>());
        }
        assert_eq!(notes, 9, "of the texts, the notes");
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

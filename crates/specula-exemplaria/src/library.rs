//! The library's entries: the kind files under `exemplaria/` at the repository's root, by plan
//! (`exemplaria/<plan>/<name>.kind.json`), built into the engine so that what it answers and
//! what a test hashes are the same bytes. A test holds this table against the folder: a file
//! there without its line here, or a line without its file, fails.
//!
//! **Adding a kind**: its file under `exemplaria/<plan>/`, and one line in [`ENTRIES`].

use crate::error::Error;
use crate::kind::Kind;
use crate::plan::{resolve, Resolved};
use crate::words;
use specula_core::hash::StableHasher;

/// A kind file of the library.
#[derive(Debug)]
pub struct Entry {
    /// Its path under `exemplaria/` (`box_creature/box_creature.kind.json`).
    pub path: &'static str,
    pub text: &'static str,
}

/// Every kind of the library.
pub static ENTRIES: &[Entry] = &[
    Entry { path: "box_creature/box_creature.kind.json", text: include_str!("../../../exemplaria/box_creature/box_creature.kind.json") },
];

impl Entry {
    /// The kind's name (its file's name without `.kind.json`).
    pub fn name(&self) -> &'static str {
        let file = self.path.rsplit('/').next().unwrap_or(self.path);
        file.strip_suffix(".kind.json").unwrap_or(file)
    }

    /// A hash of its text: what a project's copy records it was taken from.
    pub fn hash(&self) -> u64 {
        let mut h = StableHasher::with_seed(0x6578_656d_706c_6172);
        h.write_str(self.text);
        h.finish()
    }

    /// `exemplaria/<plan>/<name>@<hash>`: where a copy came from.
    pub fn from(&self) -> String {
        format!("exemplaria/{}@{:016x}", self.path.strip_suffix(".kind.json").unwrap_or(self.path), self.hash())
    }

    pub fn kind(&self) -> Result<Kind, Error> {
        Kind::parse(self.text).map_err(|e| Error { message: format!("exemplaria/{}: {}", self.path, e.message), ..e })
    }

    /// The kind with its base (from the library), checked against its plan.
    pub fn resolved(&self) -> Result<Resolved, Error> {
        let kind = self.kind()?;
        let base = match &kind.base {
            Some(b) => Some(entry(b).ok_or_else(|| Error::new("base_not_found", "base", format!("the library has no kind '{b}'")))?.kind()?),
            None => None,
        };
        resolve(&kind, base.as_ref())
    }
}

/// A hash of every entry: the library's version, which a project's copy may be compared with.
pub fn version() -> u64 {
    let mut h = StableHasher::with_seed(0x6578_656d_706c_6172);
    for e in ENTRIES {
        h.write_str(e.path);
        h.write_u64(e.hash());
    }
    h.finish()
}

/// The entry of this exact name.
pub fn entry(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.name() == name)
}

/// The entry a word names: its name, or one of its aliases (an alias is read from the file).
pub fn find(word: &str) -> Option<&'static Entry> {
    entry(word).or_else(|| ENTRIES.iter().find(|e| e.kind().is_ok_and(|k| k.aliases.iter().any(|a| a == word))))
}

/// Every name and alias of the library, with the entry each names.
pub fn names() -> Vec<(String, &'static Entry)> {
    let mut out = Vec::new();
    for e in ENTRIES {
        out.push((e.name().to_string(), e));
        if let Ok(k) = e.kind() {
            out.extend(k.aliases.into_iter().map(|a| (a, e)));
        }
    }
    out
}

/// Up to `n` names and aliases of the library nearest to `word`.
pub fn nearest(word: &str, n: usize) -> Vec<String> {
    let all = names();
    words::nearest(word, all.iter().map(|(name, _)| name.as_str()), n)
}

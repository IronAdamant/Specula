//! Specula Ludus (*ludus*: a game, play): what a game is besides its rules, as data.
//!
//! A game built from modules and data has rules (its modules, its Lex) and a world. This crate
//! holds the rest of what makes it a game a person can sit down to, as text an agent edits and
//! the engine checks (docs/design/FIRST_HOUR.md):
//!
//! - [`game`]: the game file, `game.json`: its title, its tick rate, the modules it is made of,
//!   which screens play which part, the settings it has, and when a run is won or lost.
//! - [`cond`]: the conditions that say so, over what the game keeps.
//! - [`settings`]: what a player may set, and the values they chose, as the file that keeps them.
//! - [`action`]: what a menu's button does, as a word.
//! - [`run`]: a run of the game: where it stands (title, running, paused, won, lost), which
//!   screens are up, and what a press or the end of a tick leads to.
//!
//! Pure data and pure functions: no files, no clock, no window. Deterministic, so that a run
//! ends on the same tick wherever it is replayed.
#![forbid(unsafe_code)]

pub mod action;
pub mod cond;
pub mod game;
pub mod run;
pub mod settings;

pub use action::Action;
pub use cond::{Cond, Lookup, Val};
pub use game::{Ended, GameFile, Outcome, Screens, Start};
pub use run::{Effect, Phase, Refused, Run};
pub use settings::{Kept, Setting, SettingKind, Settings};

/// What is wrong with a piece of data, as data: where, a code to branch on, and what to write
/// instead (AGENTS.md rule 5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// Where in the file: `settings[2].min`, `outcome.won.all[0].key`.
    pub at: String,
    /// A stable word for the kind of problem.
    pub code: &'static str,
    /// What is wrong and what to write.
    pub message: String,
}

impl Problem {
    pub fn new(at: &str, code: &'static str, message: impl Into<String>) -> Problem {
        Problem { at: at.to_owned(), code, message: message.into() }
    }

    pub fn to_json(&self) -> specula_core::Json {
        use specula_core::Json;
        Json::obj([("at", Json::Str(self.at.clone())), ("code", Json::Str(self.code.to_owned())), ("message", Json::Str(self.message.clone()))])
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({})", self.at, self.message, self.code)
    }
}

impl std::error::Error for Problem {}

/// A name as files and keys use it: a lowercase letter, then lowercase letters, digits and `_`,
/// at most `max` characters.
pub(crate) fn name_ok(name: &str, max: usize) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && name.len() <= max && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The keys of an object that are not among `known`: an unknown key is refused by name, with
/// the ones there are.
pub(crate) fn unknown_keys(json: &specula_core::Json, known: &[&str], at: &str) -> Result<(), Problem> {
    let Some(fields) = json.as_object() else {
        return Err(Problem::new(at, "not_an_object", "expected an object: { ... }"));
    };
    match fields.iter().find(|(k, _)| !known.contains(&k.as_str())) {
        Some((key, _)) => {
            let here = if at.is_empty() { key.clone() } else { format!("{at}.{key}") };
            Err(Problem::new(&here, "unknown_key", format!("\"{key}\" is not a key here; there are: {}", known.join(", "))))
        }
        None => Ok(()),
    }
}

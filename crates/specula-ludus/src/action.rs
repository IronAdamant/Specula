//! What a menu's button does, as a word.
//!
//! A screen's button carries an `action`: plain text. The project shell knows the words below,
//! so a title screen, a pause menu and a settings screen need no code. **Any other word is the
//! game's**: it reaches the game's modules as an event at the next tick.
//!
//! | Word | Does |
//! |---|---|
//! | `open:NAME`, `goto:NAME`, `close`, `back` | the screen stack: put one on top, replace the top, take the top off |
//! | `start`, `pause`, `resume`, `title` | the run |
//! | `set:NAME:VALUE`, `step:NAME:+1`, `step:NAME:-1`, `cycle:NAME` | a setting |
//! | `save:SLOT`, `load:SLOT` | save games |
//! | `quit` | the player's alone |

use crate::name_ok;

/// Most characters in an action word.
pub const MAX_ACTION: usize = 128;

/// A button's action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Put the screen on top of the ones that are up.
    Open(String),
    /// Take the top screen off and put this one in its place.
    Goto(String),
    /// Take the top screen off.
    Close,
    /// What the screen's own `back` says, or [`Action::Close`].
    Back,
    /// Begin the run (from the title).
    Start,
    Pause,
    Resume,
    /// End the run and go to the title.
    Title,
    /// Give a setting a value, as text (the setting's kind says how it is read).
    Set { name: String, value: String },
    /// Move a setting one step: up (`+1`) or down (`-1`); it stops at its ends.
    Step { name: String, up: bool },
    /// Move a setting to its next value, and round to the first after the last.
    Cycle(String),
    Save(String),
    Load(String),
    /// Close the game: the player's alone, never an agent's.
    Quit,
    /// Not the shell's: the game's own word, handed to its modules.
    Game(String),
}

/// A save slot's name: 1 to 32 of letters, digits, `_` and `-`.
fn slot_ok(slot: &str) -> bool {
    !slot.is_empty() && slot.len() <= 32 && slot.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl Action {
    /// Reads a button's action. A word the shell knows with a part that is wrong (`open:` with
    /// no name, `step:volume:+2`) is an error, which says what the word takes: it is not handed
    /// to the game as if it were the game's.
    pub fn parse(word: &str) -> Result<Action, String> {
        if word.is_empty() || word.chars().count() > MAX_ACTION || word.chars().any(|c| c.is_control()) {
            return Err(format!("an action is 1 to {MAX_ACTION} characters"));
        }
        let (head, rest) = match word.split_once(':') {
            Some((head, rest)) => (head, Some(rest)),
            None => (word, None),
        };
        let screen = |rest: Option<&str>| match rest {
            Some(name) if name_ok(name, 64) => Ok(name.to_owned()),
            _ => Err(format!("\"{head}\" takes a screen's name: {head}:settings")),
        };
        let setting = |name: &str| if name_ok(name, 32) { Ok(name.to_owned()) } else { Err(format!("\"{name}\" is not a setting's name (a lowercase letter, then letters, digits and _)")) };
        let alone = |action: Action| match rest {
            None => Ok(action),
            Some(_) => Err(format!("\"{head}\" takes nothing after it")),
        };
        match head {
            "open" => Ok(Action::Open(screen(rest)?)),
            "goto" => Ok(Action::Goto(screen(rest)?)),
            "close" => alone(Action::Close),
            "back" => alone(Action::Back),
            "start" => alone(Action::Start),
            "pause" => alone(Action::Pause),
            "resume" => alone(Action::Resume),
            "title" => alone(Action::Title),
            "quit" => alone(Action::Quit),
            "set" => match rest.and_then(|r| r.split_once(':')) {
                Some((name, value)) if !value.is_empty() => Ok(Action::Set { name: setting(name)?, value: value.to_owned() }),
                _ => Err("\"set\" takes a setting and a value: set:difficulty:hard".to_owned()),
            },
            "step" => match rest.and_then(|r| r.split_once(':')) {
                Some((name, "+1")) => Ok(Action::Step { name: setting(name)?, up: true }),
                Some((name, "-1")) => Ok(Action::Step { name: setting(name)?, up: false }),
                _ => Err("\"step\" takes a setting and +1 or -1: step:volume:+1".to_owned()),
            },
            "cycle" => match rest {
                Some(name) => Ok(Action::Cycle(setting(name)?)),
                None => Err("\"cycle\" takes a setting: cycle:language".to_owned()),
            },
            "save" | "load" => match rest {
                Some(slot) if slot_ok(slot) => Ok(if head == "save" { Action::Save(slot.to_owned()) } else { Action::Load(slot.to_owned()) }),
                _ => Err(format!("\"{head}\" takes a slot's name (letters, digits, _ and -): {head}:slot1")),
            },
            _ => Ok(Action::Game(word.to_owned())),
        }
    }

    /// Whether only the player may do it (an agent's press is refused and told to the input
    /// monitor, as a hosted game's is).
    pub fn players_alone(&self) -> bool {
        matches!(self, Action::Quit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shells_words() {
        let p = |w: &str| Action::parse(w).unwrap();
        assert_eq!(p("open:settings"), Action::Open("settings".into()));
        assert_eq!(p("goto:title"), Action::Goto("title".into()));
        assert_eq!((p("close"), p("back"), p("start"), p("pause"), p("resume"), p("title")), (Action::Close, Action::Back, Action::Start, Action::Pause, Action::Resume, Action::Title));
        assert_eq!(p("set:difficulty:hard"), Action::Set { name: "difficulty".into(), value: "hard".into() });
        assert_eq!(p("set:label:a:b"), Action::Set { name: "label".into(), value: "a:b".into() }, "a value may hold a colon");
        assert_eq!(p("step:volume:+1"), Action::Step { name: "volume".into(), up: true });
        assert_eq!(p("step:volume:-1"), Action::Step { name: "volume".into(), up: false });
        assert_eq!(p("cycle:language"), Action::Cycle("language".into()));
        assert_eq!((p("save:slot-1"), p("load:slot-1")), (Action::Save("slot-1".into()), Action::Load("slot-1".into())));
        assert!(p("quit").players_alone() && !p("start").players_alone());
    }

    #[test]
    fn any_other_word_is_the_games() {
        for word in ["fire", "build_wall", "shop:buy:3", "Start", "opened"] {
            assert_eq!(Action::parse(word), Ok(Action::Game(word.to_owned())), "{word}");
        }
    }

    /// A word of the shell's with a part that is wrong is an error, not the game's word.
    #[test]
    fn a_shell_word_said_wrongly_says_what_it_takes() {
        for (word, part) in [
            ("open", "open:settings"),
            ("open:", "open:settings"),
            ("open:My Screen", "open:settings"),
            ("close:now", "nothing after"),
            ("set:volume", "set:difficulty:hard"),
            ("set:volume:", "set:difficulty:hard"),
            ("set:Volume:3", "not a setting's name"),
            ("step:volume:+2", "+1 or -1"),
            ("step:volume", "+1 or -1"),
            ("cycle", "cycle:language"),
            ("save", "slot"),
            ("load:a/b", "slot"),
        ] {
            let said = Action::parse(word).unwrap_err();
            assert!(said.contains(part), "{word}: {said}");
        }
        assert!(Action::parse("").is_err() && Action::parse(&"x".repeat(MAX_ACTION + 1)).is_err() && Action::parse("a\nb").is_err());
    }
}

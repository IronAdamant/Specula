//! A run of the game: where it stands, which screens are up, and what a press does.
//!
//! This is the project shell's logic with the engine taken out: no window, no clock, no files.
//! The shell feeds it what happened (a button's [`Action`], the end of a tick) and does what it
//! answers ([`Effect`]): stop the clock, tell the modules, save. So the rules of a menu are
//! tested here in microseconds, and are the same for every game.
//!
//! ```text
//!            start                 pause
//!   Title ----------> Running <-----------> Paused
//!     ^                  |      resume         |
//!     |  title           | won or lost         | title
//!     +------------- Ended <-------------------+
//!            (start begins a new run)
//! ```
//!
//! **The modules are told, at a tick boundary, what the rules must know:** `run` with `phase`
//! `start` when a run begins and `title` when one is given up (a module resets what it keeps
//! on `start`), `ui` for a button that is the game's own, `setting` for a setting the rules
//! read, `outcome` when the run ends.

use crate::cond::{Lookup, Val};
use crate::game::{Ended, GameFile, Outcome, Screens, Start};
use crate::{Action, Settings};

/// Most screens open above the base ones at once.
pub const MAX_STACK: usize = 8;

/// Where a run stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The title screen holds the clock.
    Title,
    Running,
    Paused,
    /// Won or lost; the clock is stopped.
    Ended(Ended),
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Title => "title",
            Phase::Running => "running",
            Phase::Paused => "paused",
            Phase::Ended(how) => how.name(),
        }
    }
}

/// What the shell does after a press or a tick.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Stop (`true`) or start the engine's clock, as the game's own doing.
    Clock(bool),
    /// An event for the game's modules at the next tick boundary: its name and its data, as
    /// (key, value) pairs.
    Event { name: &'static str, data: Vec<(&'static str, Val)> },
    /// A setting has a new value: keep the file, apply what it stands for (the language, the
    /// volume).
    Setting { name: String },
    Save(String),
    Load(String),
    /// Close the game (the player asked).
    Quit,
}

/// A press that was not done, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// `players_only`, `use_save_tools`, `not_now`, `no_screen`, `bad_setting`, `too_many`.
    pub code: &'static str,
    pub message: String,
}

impl Refused {
    fn new(code: &'static str, message: impl Into<String>) -> Refused {
        Refused { code, message: message.into() }
    }
}

/// A run.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    screens: Screens,
    outcome: Outcome,
    phase: Phase,
    /// Screens opened above the base ones, the top last.
    stack: Vec<String>,
    /// The tick the run ended on.
    ended_at: Option<u64>,
    /// Runs begun so far.
    runs: u64,
}

fn event(name: &'static str, data: Vec<(&'static str, Val)>) -> Effect {
    Effect::Event { name, data }
}

fn text(s: &str) -> Val {
    Val::Text(s.to_owned())
}

impl Run {
    /// A game that has just been opened. With `"start": "run"` the run is under way at once,
    /// and [`Run::began`] says what the shell does for that.
    pub fn new(game: &GameFile) -> Run {
        let phase = if game.start == Start::Run { Phase::Running } else { Phase::Title };
        Run { screens: game.screens.clone(), outcome: game.outcome.clone(), phase, stack: Vec::new(), ended_at: None, runs: u64::from(phase == Phase::Running) }
    }

    /// What the shell does when the game opens: hold the clock at the title, or begin the run.
    pub fn began(&self, settings: &Settings) -> Vec<Effect> {
        match self.phase {
            Phase::Running => self.start_effects(settings),
            _ => vec![Effect::Clock(true)],
        }
    }

    /// The game file changed while the game runs: its screens' parts and its outcome are the
    /// new ones; where the run stands is kept.
    pub fn reconfigure(&mut self, game: &GameFile) {
        self.screens = game.screens.clone();
        self.outcome = game.outcome.clone();
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The tick the run ended on, if it has.
    pub fn ended_at(&self) -> Option<u64> {
        self.ended_at
    }

    /// Runs begun since the game was opened.
    pub fn runs(&self) -> u64 {
        self.runs
    }

    /// The screens that are up, the bottom first: the phase's own, then the ones opened.
    pub fn visible(&self) -> Vec<&str> {
        let s = &self.screens;
        let mut up: Vec<&str> = match self.phase {
            Phase::Title => vec![&s.title],
            Phase::Running => vec![&s.hud],
            Phase::Paused => vec![&s.hud, &s.pause],
            Phase::Ended(Ended::Won) => vec![&s.hud, &s.won],
            Phase::Ended(_) => vec![&s.hud, &s.lost],
        };
        up.extend(self.stack.iter().map(String::as_str));
        up
    }

    /// The screens opened above the phase's own.
    pub fn stack(&self) -> &[String] {
        &self.stack
    }

    /// The modules are told the settings the rules read, then that the run starts.
    fn start_effects(&self, settings: &Settings) -> Vec<Effect> {
        let mut out = vec![Effect::Clock(false)];
        out.extend(settings.sim_values().into_iter().map(|(name, value)| event("setting", vec![("name", text(name)), ("value", value.clone())])));
        out.push(event("run", vec![("phase", text("start")), ("run", Val::Num(self.runs as f64))]));
        out
    }

    /// A button was pressed. `player`: by the person at the window (an agent's press is refused
    /// what only the player may do). `has_screen`: whether the project has a screen of that
    /// name. A press that is refused changes nothing.
    pub fn act(&mut self, action: &Action, settings: &mut Settings, player: bool, has_screen: &dyn Fn(&str) -> bool) -> Result<Vec<Effect>, Refused> {
        if action.players_alone() && !player {
            return Err(Refused::new("players_only", "only the player closes the game: an agent's press is not taken"));
        }
        // The game's own save and load are the player's menu: they write and replace whatever the
        // slot and the world hold, as the person at the window may. An agent has the save tools,
        // which hold it to its own saves and to what others have not saved (review R1 of M6
        // part 3: through a button's word an agent overwrote another's save and loaded over
        // unsaved work). Not a try at the player's machine, so no matter for the input monitor.
        if !player && matches!(action, Action::Save(_) | Action::Load(_)) {
            return Err(Refused::new("use_save_tools", "the game's own save and load are the player's: an agent saves and loads with save_write and save_read"));
        }
        let not_now = |what: &str, phase: Phase| Refused::new("not_now", format!("{what} (the game is at: {})", phase.name()));
        match action {
            Action::Open(name) | Action::Goto(name) => {
                if !has_screen(name) {
                    return Err(Refused::new("no_screen", format!("the project has no screen \"{name}\" (assets/ui/{name}.json)")));
                }
                let goto = matches!(action, Action::Goto(_));
                if goto && !self.stack.is_empty() {
                    self.stack.pop();
                }
                if self.stack.last() != Some(name) {
                    if self.stack.len() >= MAX_STACK {
                        return Err(Refused::new("too_many", format!("at most {MAX_STACK} screens are open above the game at once")));
                    }
                    self.stack.push(name.clone());
                }
                Ok(Vec::new())
            }
            Action::Close | Action::Back if !self.stack.is_empty() => {
                self.stack.pop();
                Ok(Vec::new())
            }
            // Back with nothing open: the pause menu while playing, and out of it again.
            Action::Back => match self.phase {
                Phase::Running => self.act(&Action::Pause, settings, player, has_screen),
                Phase::Paused => self.act(&Action::Resume, settings, player, has_screen),
                Phase::Title | Phase::Ended(_) => Ok(Vec::new()),
            },
            Action::Close => Ok(Vec::new()),
            Action::Start => match self.phase {
                Phase::Title | Phase::Ended(_) => {
                    self.phase = Phase::Running;
                    self.stack.clear();
                    self.ended_at = None;
                    self.runs += 1;
                    Ok(self.start_effects(settings))
                }
                phase => Err(not_now("a run is under way: \"title\" gives it up first", phase)),
            },
            Action::Pause => match self.phase {
                Phase::Running => {
                    self.phase = Phase::Paused;
                    Ok(vec![Effect::Clock(true)])
                }
                phase => Err(not_now("there is no run to pause", phase)),
            },
            Action::Resume => match self.phase {
                Phase::Paused => {
                    self.phase = Phase::Running;
                    self.stack.clear();
                    Ok(vec![Effect::Clock(false)])
                }
                phase => Err(not_now("the game is not paused", phase)),
            },
            Action::Title => match self.phase {
                Phase::Title => Err(not_now("the game is at its title already", Phase::Title)),
                _ => {
                    self.phase = Phase::Title;
                    self.stack.clear();
                    self.ended_at = None;
                    Ok(vec![Effect::Clock(true), event("run", vec![("phase", text("title"))])])
                }
            },
            Action::Set { .. } | Action::Step { .. } | Action::Cycle(_) => {
                let (name, changed) = match action {
                    Action::Set { name, value } => (name, settings.set_text(name, value)),
                    Action::Step { name, up } => (name, settings.step(name, *up)),
                    Action::Cycle(name) => (name, settings.cycle(name)),
                    _ => unreachable!("matched above"),
                };
                if !changed.map_err(|why| Refused::new("bad_setting", why))? {
                    return Ok(Vec::new());
                }
                let mut out = vec![Effect::Setting { name: name.clone() }];
                // A setting the rules read reaches them while a run is under way; a run that
                // has not begun is told all of them when it does.
                if settings.is_sim(name) && matches!(self.phase, Phase::Running | Phase::Paused) {
                    if let Some(value) = settings.get(name) {
                        out.push(event("setting", vec![("name", text(name)), ("value", value.clone())]));
                    }
                }
                Ok(out)
            }
            Action::Save(slot) => match self.phase {
                Phase::Running | Phase::Paused => Ok(vec![Effect::Save(slot.clone())]),
                phase => Err(not_now("there is no run to save", phase)),
            },
            Action::Load(slot) => Ok(vec![Effect::Load(slot.clone())]),
            Action::Quit => Ok(vec![Effect::Quit]),
            Action::Game(word) => Ok(vec![event("ui", vec![("action", text(word))])]),
        }
    }

    /// A tick ended. A run that is under way ends when its outcome's conditions say so.
    pub fn after_tick(&mut self, look: &dyn Lookup, tick: u64) -> Vec<Effect> {
        if self.phase != Phase::Running {
            return Vec::new();
        }
        let how = self.outcome.after_tick(look);
        if how == Ended::Running {
            return Vec::new();
        }
        self.phase = Phase::Ended(how);
        self.stack.clear();
        self.ended_at = Some(tick);
        vec![Effect::Clock(true), event("outcome", vec![("outcome", text(how.name())), ("tick", Val::Num(tick as f64))])]
    }

    /// A save game was loaded: the run is where the save was (`phase`, and the tick it ended
    /// on), with nothing open above it.
    pub fn restore(&mut self, phase: Phase, ended_at: Option<u64>, runs: u64) {
        self.runs = runs;
        self.phase = phase;
        self.ended_at = ended_at;
        self.stack.clear();
    }

    /// The engine's clock was started or stopped by someone else (an agent's `time_control`):
    /// a paused game whose clock runs is running, and the pause menu goes. Nothing else follows
    /// the clock: an agent that steps a game at its title steps a game at its title.
    pub fn clock_seen(&mut self, paused: bool) {
        if self.phase == Phase::Paused && !paused {
            self.phase = Phase::Running;
            self.stack.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::GameFile;

    const GAME: &str = r#"{
        "format": "specula.game.v1",
        "settings": [
            { "name": "difficulty", "kind": "choice", "options": ["easy", "hard"], "sim": true },
            { "name": "hints", "kind": "toggle" }
        ],
        "outcome": { "lost": { "key": "gate.hp", "lte": 0 }, "won": "gate.won" }
    }"#;

    fn game() -> (Run, Settings) {
        let file = GameFile::parse(GAME).unwrap();
        (Run::new(&file), Settings::new(&file.settings, &["en".to_owned(), "es".to_owned()]))
    }

    fn press(run: &mut Run, settings: &mut Settings, word: &str) -> Result<Vec<Effect>, Refused> {
        run.act(&Action::parse(word).unwrap(), settings, true, &|name: &str| name != "nowhere")
    }

    fn hp(n: f64) -> impl Fn(&str) -> Option<Val> {
        move |key: &str| (key == "gate.hp").then_some(Val::Num(n))
    }

    #[test]
    fn a_game_opens_at_its_title_with_the_clock_held() {
        let (mut run, mut s) = game();
        assert_eq!((run.phase(), run.visible(), run.runs()), (Phase::Title, vec!["title"], 0));
        assert_eq!(run.began(&s), [Effect::Clock(true)]);
        // Start: the clock runs, the modules are told the settings the rules read, then `run`.
        let effects = press(&mut run, &mut s, "start").unwrap();
        assert_eq!(
            effects,
            [
                Effect::Clock(false),
                event("setting", vec![("name", text("difficulty")), ("value", text("easy"))]),
                event("run", vec![("phase", text("start")), ("run", Val::Num(1.0))]),
            ]
        );
        assert_eq!((run.phase(), run.visible()), (Phase::Running, vec!["hud"]));
        assert_eq!(press(&mut run, &mut s, "start").unwrap_err().code, "not_now");
    }

    #[test]
    fn a_game_that_starts_in_its_run_does() {
        let file = GameFile::parse(r#"{"format": "specula.game.v1", "start": "run"}"#).unwrap();
        let run = Run::new(&file);
        assert_eq!((run.phase(), run.runs()), (Phase::Running, 1));
        let effects = run.began(&Settings::new(&[], &[]));
        assert_eq!(effects, [Effect::Clock(false), event("run", vec![("phase", text("start")), ("run", Val::Num(1.0))])]);
    }

    #[test]
    fn pause_stops_the_clock_and_resume_starts_it() {
        let (mut run, mut s) = game();
        assert_eq!(press(&mut run, &mut s, "pause").unwrap_err().code, "not_now", "no run at the title");
        press(&mut run, &mut s, "start").unwrap();
        assert_eq!(press(&mut run, &mut s, "pause"), Ok(vec![Effect::Clock(true)]));
        assert_eq!((run.phase(), run.visible()), (Phase::Paused, vec!["hud", "pause"]));
        // Settings opened from the pause menu, and closed again.
        assert_eq!(press(&mut run, &mut s, "open:settings"), Ok(vec![]));
        assert_eq!(run.visible(), ["hud", "pause", "settings"]);
        assert_eq!(press(&mut run, &mut s, "back"), Ok(vec![]));
        assert_eq!(run.visible(), ["hud", "pause"]);
        // Back with nothing open leaves the pause menu; back while playing opens it.
        assert_eq!(press(&mut run, &mut s, "back"), Ok(vec![Effect::Clock(false)]));
        assert_eq!(run.phase(), Phase::Running);
        assert_eq!(press(&mut run, &mut s, "back"), Ok(vec![Effect::Clock(true)]));
        assert_eq!(run.phase(), Phase::Paused);
        // Resume closes whatever is open above the pause menu too.
        press(&mut run, &mut s, "open:settings").unwrap();
        assert_eq!(press(&mut run, &mut s, "resume"), Ok(vec![Effect::Clock(false)]));
        assert_eq!(run.visible(), ["hud"]);
    }

    #[test]
    fn a_run_ends_on_the_tick_its_condition_holds() {
        let (mut run, mut s) = game();
        assert!(run.after_tick(&hp(0.0), 3).is_empty(), "no run is under way at the title");
        press(&mut run, &mut s, "start").unwrap();
        assert!(run.after_tick(&hp(2.0), 10).is_empty());
        let effects = run.after_tick(&hp(0.0), 11);
        assert_eq!(effects, [Effect::Clock(true), event("outcome", vec![("outcome", text("lost")), ("tick", Val::Num(11.0))])]);
        assert_eq!((run.phase(), run.ended_at(), run.visible()), (Phase::Ended(Ended::Lost), Some(11), vec!["hud", "lost"]));
        assert!(run.after_tick(&hp(0.0), 12).is_empty(), "it ends once");
        // A new run from the end screen.
        let effects = press(&mut run, &mut s, "start").unwrap();
        assert_eq!(effects.last(), Some(&event("run", vec![("phase", text("start")), ("run", Val::Num(2.0))])));
        assert_eq!((run.phase(), run.ended_at(), run.runs()), (Phase::Running, None, 2));
        // Won.
        let won = |key: &str| (key == "gate.won").then_some(Val::Bool(true));
        run.after_tick(&won, 40);
        assert_eq!(run.visible(), ["hud", "won"]);
        // And back to the title: the modules are told the run was given up.
        assert_eq!(press(&mut run, &mut s, "title"), Ok(vec![Effect::Clock(true), event("run", vec![("phase", text("title"))])]));
        assert_eq!((run.phase(), run.visible()), (Phase::Title, vec!["title"]));
        assert_eq!(press(&mut run, &mut s, "title").unwrap_err().code, "not_now");
    }

    #[test]
    fn settings_change_from_buttons_and_the_rules_hear_of_theirs() {
        let (mut run, mut s) = game();
        // At the title: kept, and the rules are told when the run begins.
        assert_eq!(press(&mut run, &mut s, "cycle:difficulty"), Ok(vec![Effect::Setting { name: "difficulty".into() }]));
        assert_eq!(press(&mut run, &mut s, "set:difficulty:hard"), Ok(vec![]), "it had that value: nothing to do");
        let effects = press(&mut run, &mut s, "start").unwrap();
        assert!(effects.contains(&event("setting", vec![("name", text("difficulty")), ("value", text("hard"))])));
        // In a run: told at once (the shell hands it to the next tick).
        let effects = press(&mut run, &mut s, "cycle:difficulty").unwrap();
        assert_eq!(effects, [Effect::Setting { name: "difficulty".into() }, event("setting", vec![("name", text("difficulty")), ("value", text("easy"))])]);
        // A setting the rules do not read is the shell's alone.
        assert_eq!(press(&mut run, &mut s, "cycle:hints"), Ok(vec![Effect::Setting { name: "hints".into() }]));
        assert_eq!(press(&mut run, &mut s, "step:volume:+1"), Ok(vec![Effect::Setting { name: "volume".into() }]));
        assert_eq!(press(&mut run, &mut s, "cycle:language"), Ok(vec![Effect::Setting { name: "language".into() }]));
        assert_eq!(s.locale(), "es");
        assert_eq!(press(&mut run, &mut s, "set:volume:99").unwrap_err().code, "bad_setting");
        assert_eq!(press(&mut run, &mut s, "cycle:nothing").unwrap_err().code, "bad_setting");
    }

    #[test]
    fn the_games_own_words_go_to_its_modules() {
        let (mut run, mut s) = game();
        assert_eq!(press(&mut run, &mut s, "build_wall"), Ok(vec![event("ui", vec![("action", text("build_wall"))])]));
        assert_eq!(press(&mut run, &mut s, "shop:buy:3"), Ok(vec![event("ui", vec![("action", text("shop:buy:3"))])]));
    }

    #[test]
    fn what_is_refused_changes_nothing() {
        let (mut run, mut s) = game();
        let before = (run.clone(), s.clone());
        assert_eq!(press(&mut run, &mut s, "open:nowhere").unwrap_err().code, "no_screen");
        assert_eq!(press(&mut run, &mut s, "save:slot1").unwrap_err().code, "not_now");
        // Quit is the player's alone.
        let quit = Action::parse("quit").unwrap();
        assert_eq!(run.act(&quit, &mut s, false, &|_| true).unwrap_err().code, "players_only");
        assert_eq!((run.clone(), s.clone()), before);
        assert_eq!(run.act(&quit, &mut s, true, &|_| true), Ok(vec![Effect::Quit]));
        // The stack is bounded, and opening the screen that is on top opens nothing.
        press(&mut run, &mut s, "open:settings").unwrap();
        press(&mut run, &mut s, "open:settings").unwrap();
        assert_eq!(run.stack().len(), 1);
        for i in 1..MAX_STACK {
            press(&mut run, &mut s, &format!("open:s{i}")).unwrap();
        }
        assert_eq!(press(&mut run, &mut s, "open:one_more").unwrap_err().code, "too_many");
        // Goto replaces the top.
        press(&mut run, &mut s, "goto:credits").unwrap();
        assert_eq!((run.stack().len(), run.stack().last().map(String::as_str)), (MAX_STACK, Some("credits")));
        // Saves in a run.
        let (mut run, mut s) = game();
        press(&mut run, &mut s, "start").unwrap();
        assert_eq!(press(&mut run, &mut s, "save:slot1"), Ok(vec![Effect::Save("slot1".into())]));
        assert_eq!(press(&mut run, &mut s, "load:slot1"), Ok(vec![Effect::Load("slot1".into())]));
    }

    #[test]
    fn an_agent_that_starts_the_clock_of_a_paused_game_ends_the_pause() {
        let (mut run, mut s) = game();
        run.clock_seen(false);
        assert_eq!(run.phase(), Phase::Title, "a game at its title stays there, stepped or not");
        press(&mut run, &mut s, "start").unwrap();
        press(&mut run, &mut s, "pause").unwrap();
        run.clock_seen(true);
        assert_eq!(run.phase(), Phase::Paused);
        run.clock_seen(false);
        assert_eq!((run.phase(), run.visible()), (Phase::Running, vec!["hud"]));
        // A save loaded: the run is where the save was.
        run.restore(Phase::Ended(Ended::Won), Some(90), 3);
        assert_eq!((run.phase(), run.ended_at(), run.runs(), run.visible()), (Phase::Ended(Ended::Won), Some(90), 3, vec!["hud", "won"]));
        // The game's own save and load are the player's: an agent is sent to the save tools, and
        // nothing is changed or asked of the engine.
        run.restore(Phase::Running, None, 3);
        let before = (run.clone(), s.clone());
        for word in ["save:slot1", "load:slot1"] {
            let action = Action::parse(word).unwrap();
            assert_eq!(run.act(&action, &mut s, false, &|_| true).unwrap_err().code, "use_save_tools", "{word}");
            assert_eq!((run.clone(), s.clone()), before);
        }
    }
}

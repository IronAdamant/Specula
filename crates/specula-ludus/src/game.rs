//! The game file: `game.json` at a project's root.
//!
//! ```json
//! {
//!   "format": "specula.game.v1",
//!   "title": "Hold the gate",
//!   "tick_hz": 60,
//!   "modules": ["modules/gate"],
//!   "start": "title",
//!   "screens": { "title": "title", "pause": "pause", "won": "won", "lost": "lost", "hud": "hud" },
//!   "locale": "en",
//!   "settings": [ { "name": "difficulty", "kind": "choice", "options": ["easy", "hard"], "sim": true } ],
//!   "outcome": { "lost": { "key": "gate.hp", "lte": 0 }, "won": "gate.won", "score": "gate.score" }
//! }
//! ```
//!
//! Everything has a default, so the smallest game file is `{"format": "specula.game.v1"}`. An
//! unknown key is refused by name: a misspelt key is not a setting silently left at its default.

use specula_core::Json;

use crate::cond::{key_ok, Cond};
use crate::settings::{Setting, SettingKind, MAX_SETTINGS};
use crate::{name_ok, unknown_keys, Problem};

pub const FORMAT: &str = "specula.game.v1";
/// The file's name at a project's root.
pub const FILE: &str = "game.json";
/// The tick rate when the file names none, and the rates it may name.
pub const DEFAULT_TICK_HZ: u32 = 30;
pub const MAX_TICK_HZ: u32 = 240;
/// Most modules a game is made of, and most characters in its title.
pub const MAX_MODULES: usize = 32;
pub const MAX_TITLE: usize = 80;
/// The largest game file read.
pub const MAX_BYTES: usize = 256 * 1024;

/// How a run begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Start {
    /// The title screen holds the clock until the player starts.
    #[default]
    Title,
    /// The run is under way as soon as the game is.
    Run,
}

/// Which screen plays which part. Each is the name of a screen file (`assets/ui/NAME.json`);
/// a part whose file is not there is simply not shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screens {
    pub title: String,
    pub pause: String,
    pub won: String,
    pub lost: String,
    /// Shown while a run is under way.
    pub hud: String,
}

impl Default for Screens {
    fn default() -> Screens {
        let s = |name: &str| name.to_owned();
        Screens { title: s("title"), pause: s("pause"), won: s("won"), lost: s("lost"), hud: s("hud") }
    }
}

impl Screens {
    /// (part, screen), in a fixed order.
    pub fn parts(&self) -> [(&'static str, &str); 5] {
        [("title", &self.title), ("pause", &self.pause), ("won", &self.won), ("lost", &self.lost), ("hud", &self.hud)]
    }
}

/// When a run ends, and what it scored.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Outcome {
    pub won: Option<Cond>,
    pub lost: Option<Cond>,
    /// A key that holds the score, a number.
    pub score: Option<String>,
    /// A key that holds a list of rows: a race's order, a table of players.
    pub standings: Option<String>,
}

/// How a run stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Ended {
    #[default]
    Running,
    Won,
    Lost,
}

impl Ended {
    pub fn name(self) -> &'static str {
        match self {
            Ended::Running => "running",
            Ended::Won => "won",
            Ended::Lost => "lost",
        }
    }

    pub fn named(name: &str) -> Option<Ended> {
        [Ended::Running, Ended::Won, Ended::Lost].into_iter().find(|e| e.name() == name)
    }
}

impl Outcome {
    /// How a run that is under way stands after a tick, by what `look` shows. **A run that is
    /// lost and won on the same tick is lost**: the gate fell as the last runner did.
    pub fn after_tick(&self, look: &dyn crate::Lookup) -> Ended {
        if self.lost.as_ref().is_some_and(|c| c.holds(look)) {
            Ended::Lost
        } else if self.won.as_ref().is_some_and(|c| c.holds(look)) {
            Ended::Won
        } else {
            Ended::Running
        }
    }

    /// Every key the outcome reads.
    pub fn keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.lost.iter().chain(&self.won).flat_map(Cond::keys).collect();
        keys.extend(self.score.as_deref());
        keys.extend(self.standings.as_deref());
        keys
    }
}

/// Most lines of a game's report (what a headless run prints).
pub const MAX_REPORT: usize = 64;
/// Most values a game keeps for the player ([`GameFile::keep`]).
pub const MAX_KEEP: usize = 16;
/// The longest name of a kept value: with what stands before it in the player's file
/// ([`crate::settings::KEPT`]) it is a name that file's reader takes (64 characters).
pub const MAX_KEEP_NAME: usize = 48;
/// Most screens the rules put up ([`GameFile::show`]).
pub const MAX_SHOW: usize = 8;

/// How the game is looked at: by its window with no camera of an agent's, and by a headless
/// run's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    /// Straight on, as a 2D game is seen: the point of the world at the middle of the picture
    /// and how many metres of the world its height shows.
    Flat { center: [f64; 2], height: f64 },
    /// From a point, at a point.
    Eye { eye: [f64; 3], target: [f64; 3], fov_y_deg: f64 },
}

impl View {
    fn parse(json: &Json) -> Result<View, Problem> {
        let bad = || Problem::new("view", "bad_view", "the view is {\"center\": [x, y], \"height\": metres} (straight on, as a 2D game is seen) or {\"eye\": [x, y, z], \"target\": [x, y, z], \"fov_y_deg\": degrees}");
        let numbers = |key: &str, n: usize| json.get(key).and_then(Json::as_array).filter(|a| a.len() == n).and_then(|a| a.iter().map(|x| x.as_f64().filter(|x| x.is_finite() && x.abs() <= 1.0e6)).collect::<Option<Vec<f64>>>());
        if json.get("center").is_some() {
            unknown_keys(json, &["center", "height"], "view")?;
            let center = numbers("center", 2).ok_or_else(bad)?;
            let height = json.get("height").and_then(Json::as_f64).filter(|h| h.is_finite() && (0.001..=1.0e6).contains(h)).ok_or_else(bad)?;
            return Ok(View::Flat { center: [center[0], center[1]], height });
        }
        unknown_keys(json, &["eye", "target", "fov_y_deg"], "view")?;
        let (eye, target) = (numbers("eye", 3).ok_or_else(bad)?, numbers("target", 3).ok_or_else(bad)?);
        let fov_y_deg = match json.get("fov_y_deg") {
            None => 50.0,
            Some(f) => f.as_f64().filter(|f| (1.0..=170.0).contains(f)).ok_or_else(bad)?,
        };
        if eye == target {
            return Err(bad());
        }
        Ok(View::Eye { eye: [eye[0], eye[1], eye[2]], target: [target[0], target[1], target[2]], fov_y_deg })
    }

    pub fn to_json(&self) -> Json {
        let list = |xs: &[f64]| Json::arr(xs.iter().map(|x| Json::Num(*x)));
        match self {
            View::Flat { center, height } => Json::obj([("center", list(center)), ("height", Json::Num(*height))]),
            View::Eye { eye, target, fov_y_deg } => Json::obj([("eye", list(eye)), ("target", list(target)), ("fov_y_deg", Json::Num(*fov_y_deg))]),
        }
    }
}

/// A game file.
#[derive(Clone, Debug, PartialEq)]
pub struct GameFile {
    /// The game's name as a player reads it (the window's title).
    pub title: String,
    pub tick_hz: u32,
    /// The module crates it is made of: folders below the project, with `/`.
    pub modules: Vec<String>,
    pub start: Start,
    pub screens: Screens,
    /// The language its words are written in first.
    pub locale: String,
    /// The settings it declares (the two every game has are added by [`crate::Settings::new`]).
    pub settings: Vec<Setting>,
    pub outcome: Outcome,
    /// How the game is looked at when nobody says otherwise.
    pub view: Option<View>,
    /// What a headless run prints (`specula run`): each line a name and the key whose value
    /// it shows, in this order. Empty: the run prints everything it can see.
    pub report: Vec<(String, String)>,
    /// Words a headless run's script may use beside the game's own, each with the input it
    /// stands for (`"menu_down": "down"`, `"restart": "enter"`, `"fire": "up+shoot"`): a game
    /// whose brief names its inputs is run in the brief's words, with no program between.
    pub inputs: Vec<(String, String)>,
    /// What the game keeps for the player between one run of the program and the next (a best
    /// score, a run to go on with): each a name, and the key of a module whose value is kept
    /// under it (`"best": "gate.best"`). The engine writes a value when the module changes
    /// it, beside the player's settings, and tells it to the modules when the game starts.
    pub keep: Vec<(String, String)>,
    /// Screens of the game's own that the rules put up: each a screen (a file under
    /// `assets/ui`) and the key of a module; while the run is under way and what the module
    /// keeps there is true (or a number that is not 0, or text that is not empty), the
    /// screen is up over the game's own and has the focus (`"shop": "gate.shopping"`). The
    /// clock goes on: its buttons' words reach the rules, which take it down again.
    pub show: Vec<(String, String)>,
}

impl Default for GameFile {
    fn default() -> GameFile {
        GameFile {
            title: String::new(),
            tick_hz: DEFAULT_TICK_HZ,
            modules: Vec::new(),
            start: Start::Title,
            screens: Screens::default(),
            locale: "en".to_owned(),
            settings: Vec::new(),
            outcome: Outcome::default(),
            view: None,
            report: Vec::new(),
            inputs: Vec::new(),
            keep: Vec::new(),
            show: Vec::new(),
        }
    }
}

/// A module's folder below the project: plain names joined by `/`, none of them `.` or `..`.
fn module_path_ok(path: &str) -> bool {
    !path.is_empty() && path.len() <= 200 && path.split('/').all(|part| !part.is_empty() && part != "." && part != ".." && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'))
}

/// A language tag as the string tables name them: `en`, `pt-BR`, `qps-ploc`.
fn locale_ok(tag: &str) -> bool {
    !tag.is_empty() && tag.len() <= 35 && tag.split('-').all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric()))
}

impl GameFile {
    /// Reads a game file's text.
    pub fn parse(text: &str) -> Result<GameFile, Problem> {
        if text.len() > MAX_BYTES {
            return Err(Problem::new("", "too_big", format!("a game file is at most {} KB", MAX_BYTES / 1024)));
        }
        let json = Json::parse(text).map_err(|e| Problem::new("", "not_json", format!("{FILE} is not JSON: {e}")))?;
        GameFile::from_json(&json)
    }

    pub fn from_json(json: &Json) -> Result<GameFile, Problem> {
        unknown_keys(json, &["format", "title", "tick_hz", "modules", "start", "screens", "locale", "settings", "outcome", "view", "report", "inputs", "keep", "show"], "")?;
        if json.get("format").and_then(Json::as_str) != Some(FORMAT) {
            return Err(Problem::new("format", "bad_format", format!("a game file says \"format\": \"{FORMAT}\"")));
        }
        let mut game = GameFile::default();
        if let Some(title) = json.get("title") {
            let ok = |t: &&str| !t.trim().is_empty() && t.chars().count() <= MAX_TITLE && !t.chars().any(|c| c.is_control());
            game.title = title.as_str().filter(ok).ok_or_else(|| Problem::new("title", "bad_title", format!("the title is text of 1 to {MAX_TITLE} characters")))?.to_owned();
        }
        if let Some(hz) = json.get("tick_hz") {
            let hz = hz.as_u64().filter(|n| (1..=u64::from(MAX_TICK_HZ)).contains(n)).ok_or_else(|| Problem::new("tick_hz", "bad_tick_rate", format!("the tick rate is a whole number from 1 to {MAX_TICK_HZ} (ticks a second; {DEFAULT_TICK_HZ} when left out)")))?;
            game.tick_hz = hz as u32;
        }
        if let Some(modules) = json.get("modules") {
            let list = modules.as_array().filter(|m| m.len() <= MAX_MODULES).ok_or_else(|| Problem::new("modules", "bad_modules", format!("\"modules\" is a list of at most {MAX_MODULES} folders")))?;
            for (i, m) in list.iter().enumerate() {
                let here = format!("modules[{i}]");
                let path = m.as_str().filter(|p| module_path_ok(p)).ok_or_else(|| Problem::new(&here, "bad_module", "a module is its crate's folder below the project, with /: \"modules/gate\" (no .., no drive, no backslash)"))?;
                if game.modules.iter().any(|have| have == path) {
                    return Err(Problem::new(&here, "bad_module", format!("\"{path}\" is there twice")));
                }
                game.modules.push(path.to_owned());
            }
        }
        if let Some(start) = json.get("start") {
            game.start = match start.as_str() {
                Some("title") => Start::Title,
                Some("run") => Start::Run,
                _ => return Err(Problem::new("start", "bad_start", "\"start\" is \"title\" (the title screen holds the clock until the player starts) or \"run\"")),
            };
        }
        if let Some(screens) = json.get("screens") {
            unknown_keys(screens, &["title", "pause", "won", "lost", "hud"], "screens")?;
            for (part, slot) in [("title", &mut game.screens.title), ("pause", &mut game.screens.pause), ("won", &mut game.screens.won), ("lost", &mut game.screens.lost), ("hud", &mut game.screens.hud)] {
                if let Some(name) = screens.get(part) {
                    *slot = name.as_str().filter(|n| name_ok(n, 64)).ok_or_else(|| Problem::new(&format!("screens.{part}"), "bad_screen", "a screen is named by its file under assets/ui, without .json: a lowercase letter, then lowercase letters, digits and _"))?.to_owned();
                }
            }
        }
        if let Some(locale) = json.get("locale") {
            game.locale = locale.as_str().filter(|l| locale_ok(l)).ok_or_else(|| Problem::new("locale", "bad_locale", "the locale is a language tag, as the string tables are named: \"en\", \"pt-BR\""))?.to_owned();
        }
        if let Some(settings) = json.get("settings") {
            let list = settings.as_array().filter(|s| s.len() <= MAX_SETTINGS).ok_or_else(|| Problem::new("settings", "bad_settings", format!("\"settings\" is a list of at most {MAX_SETTINGS} settings")))?;
            for (i, s) in list.iter().enumerate() {
                let here = format!("settings[{i}]");
                let setting = Setting::parse(s, &here)?;
                if game.settings.iter().any(|have| have.name == setting.name) {
                    return Err(Problem::new(&format!("{here}.name"), "bad_name", format!("\"{}\" is declared twice", setting.name)));
                }
                if setting.kind == SettingKind::Locale && game.settings.iter().any(|have| have.kind == SettingKind::Locale) {
                    return Err(Problem::new(&format!("{here}.kind"), "bad_kind", "a game has one language setting"));
                }
                if let Some(meaning) = &setting.is {
                    if game.settings.iter().any(|have| have.is.as_ref() == Some(meaning)) {
                        return Err(Problem::new(&format!("{here}.is"), "bad_meaning", format!("one setting is the {meaning}")));
                    }
                }
                game.settings.push(setting);
            }
        }
        if let Some(outcome) = json.get("outcome") {
            unknown_keys(outcome, &["won", "lost", "score", "standings"], "outcome")?;
            let cond = |part: &str| outcome.get(part).map(|c| Cond::parse(c, &format!("outcome.{part}"))).transpose();
            let key = |part: &str| match outcome.get(part) {
                None => Ok(None),
                Some(k) => k.as_str().filter(|k| key_ok(k)).map(|k| Some(k.to_owned())).ok_or_else(|| Problem::new(&format!("outcome.{part}"), "bad_key", format!("\"{part}\" names the key that holds it: \"gate.score\""))),
            };
            game.outcome = Outcome { won: cond("won")?, lost: cond("lost")?, score: key("score")?, standings: key("standings")? };
        }
        if let Some(view) = json.get("view") {
            game.view = Some(View::parse(view)?);
        }
        if let Some(report) = json.get("report") {
            let lines = report.as_object().filter(|r| r.len() <= MAX_REPORT).ok_or_else(|| Problem::new("report", "bad_report", format!("\"report\" is an object of at most {MAX_REPORT} lines: a name, and the key whose value a headless run prints under it")))?;
            for (name, key) in lines {
                let here = format!("report.{name}");
                if !key_ok(name) {
                    return Err(Problem::new(&here, "bad_name", "a report's name is 1 to 64 characters with no blank"));
                }
                let key = key.as_str().filter(|k| key_ok(k)).ok_or_else(|| Problem::new(&here, "bad_key", "a report's line names the key whose value it shows: \"gate.score\" (what a module keeps), \"setting.volume\", \"game.phase\", \"ui.texts\", \"run.ticks\""))?;
                game.report.push((name.clone(), key.to_owned()));
            }
        }
        if let Some(inputs) = json.get("inputs") {
            let words = inputs.as_object().filter(|w| w.len() <= MAX_REPORT).ok_or_else(|| Problem::new("inputs", "bad_inputs", format!("\"inputs\" is an object of at most {MAX_REPORT} words: a word a script may use, and the input it stands for (\"menu_down\": \"down\")")))?;
            let word_ok = |w: &str| key_ok(w) && !w.contains([',', ':']);
            for (word, means) in words {
                let here = format!("inputs.{word}");
                if !word_ok(word) || word.contains(['+', '=']) {
                    return Err(Problem::new(&here, "bad_name", "a word is 1 to 64 characters with no blank, comma, colon, + or ="));
                }
                let means = means.as_str().filter(|m| word_ok(m)).ok_or_else(|| Problem::new(&here, "bad_input", "a word stands for an input: \"down\", \"enter\", an action of a module, or several joined by + (\"up+shoot\")"))?;
                game.inputs.push((word.clone(), means.to_owned()));
            }
        }
        if let Some(keep) = json.get("keep") {
            let names = keep.as_object().filter(|k| k.len() <= MAX_KEEP).ok_or_else(|| Problem::new("keep", "bad_keep", format!("\"keep\" is an object of at most {MAX_KEEP} names: a name, and the key of a module whose value is kept for the player under it (\"best\": \"gate.best\")")))?;
            for (name, key) in names {
                let here = format!("keep.{name}");
                if !key_ok(name) || name.contains('.') || name.chars().count() > MAX_KEEP_NAME {
                    return Err(Problem::new(&here, "bad_name", format!("a kept value's name is 1 to {MAX_KEEP_NAME} characters with no blank and no dot")));
                }
                // (What a module keeps: the engine's own lines are not a game's to keep.)
                let of_a_module = |k: &&str| key_ok(k) && k.split_once('.').is_some_and(|(module, key)| !key.is_empty() && !["run", "ui", "game", "setting", "world"].contains(&module));
                let key = key.as_str().filter(of_a_module).ok_or_else(|| Problem::new(&here, "bad_key", "a kept value names what a module keeps: \"gate.best\" (module gate, its state key best)"))?;
                game.keep.push((name.clone(), key.to_owned()));
            }
        }
        if let Some(show) = json.get("show") {
            let screens = show.as_object().filter(|k| k.len() <= MAX_SHOW).ok_or_else(|| Problem::new("show", "bad_show", format!("\"show\" is an object of at most {MAX_SHOW} screens: a screen's name, and the key of a module that says when it is up (\"shop\": \"gate.shopping\")")))?;
            for (screen, key) in screens {
                let here = format!("show.{screen}");
                if !key_ok(screen) || screen.contains(['.', '/', '\\']) {
                    return Err(Problem::new(&here, "bad_name", "a shown screen is named as its file under assets/ui is, without .json: \"shop\""));
                }
                let of_a_module = |k: &&str| key_ok(k) && k.split_once('.').is_some_and(|(module, key)| !key.is_empty() && !["run", "ui", "game", "setting", "world"].contains(&module));
                let key = key.as_str().filter(of_a_module).ok_or_else(|| Problem::new(&here, "bad_key", "a shown screen names what a module keeps, which says when it is up: \"gate.shopping\""))?;
                game.show.push((screen.clone(), key.to_owned()));
            }
        }
        Ok(game)
    }

    /// The file's text, with every part written out.
    pub fn to_json(&self) -> Json {
        let screens = Json::obj(self.screens.parts().map(|(part, name)| (part, Json::Str(name.to_owned()))));
        let setting = |s: &Setting| {
            let mut j = Json::obj([("name", Json::Str(s.name.clone()))]);
            match &s.kind {
                SettingKind::Int { min, max } => {
                    j.set("kind", Json::Str("int".to_owned()));
                    j.set("min", Json::Num(*min as f64));
                    j.set("max", Json::Num(*max as f64));
                }
                SettingKind::Choice { options } => {
                    j.set("kind", Json::Str("choice".to_owned()));
                    j.set("options", Json::arr(options.iter().cloned().map(Json::Str)));
                }
                SettingKind::Toggle => {
                    j.set("kind", Json::Str("toggle".to_owned()));
                }
                SettingKind::Locale => {
                    j.set("kind", Json::Str("locale".to_owned()));
                }
            }
            if let (Some(default), false) = (&s.default, s.kind == SettingKind::Locale) {
                j.set("default", default.to_json());
            }
            if let Some(is) = &s.is {
                j.set("is", Json::Str(is.clone()));
            }
            if s.sim {
                j.set("sim", Json::Bool(true));
            }
            j
        };
        let mut outcome = Json::obj::<&str>([]);
        for (part, cond) in [("won", &self.outcome.won), ("lost", &self.outcome.lost)] {
            if let Some(c) = cond {
                outcome.set(part, c.to_json());
            }
        }
        for (part, key) in [("score", &self.outcome.score), ("standings", &self.outcome.standings)] {
            if let Some(k) = key {
                outcome.set(part, Json::Str(k.clone()));
            }
        }
        let file = Json::obj([
            ("format", Json::Str(FORMAT.to_owned())),
            ("title", Json::Str(self.title.clone())),
            ("tick_hz", Json::Num(f64::from(self.tick_hz))),
            ("modules", Json::arr(self.modules.iter().cloned().map(Json::Str))),
            ("start", Json::Str(if self.start == Start::Title { "title" } else { "run" }.to_owned())),
            ("screens", screens),
            ("locale", Json::Str(self.locale.clone())),
            ("settings", Json::arr(self.settings.iter().map(setting))),
            ("outcome", outcome),
        ]);
        let mut file = file;
        if let Some(view) = &self.view {
            file.set("view", view.to_json());
        }
        if !self.report.is_empty() {
            file.set("report", Json::Obj(self.report.iter().map(|(name, key)| (name.clone(), Json::Str(key.clone()))).collect()));
        }
        if !self.inputs.is_empty() {
            file.set("inputs", Json::Obj(self.inputs.iter().map(|(word, means)| (word.clone(), Json::Str(means.clone()))).collect()));
        }
        if !self.keep.is_empty() {
            file.set("keep", Json::Obj(self.keep.iter().map(|(name, key)| (name.clone(), Json::Str(key.clone()))).collect()));
        }
        if !self.show.is_empty() {
            file.set("show", Json::Obj(self.show.iter().map(|(screen, key)| (screen.clone(), Json::Str(key.clone()))).collect()));
        }
        file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cond::Val;

    /// What a game keeps for the player and the screens its rules put up are names and the
    /// keys of a module, and come back as written.
    #[test]
    fn keep_and_show_name_what_a_module_keeps() {
        let with = |more: &str| GameFile::parse(&format!(r#"{{"format": "specula.game.v1", "title": "g", {more}}}"#));
        let game = with(r#""keep": {"best": "gate.best", "run": "gate.saved.wave"}, "show": {"shop": "gate.shopping"}"#).unwrap();
        assert_eq!(game.keep, [("best".to_owned(), "gate.best".to_owned()), ("run".to_owned(), "gate.saved.wave".to_owned())]);
        assert_eq!(game.show, [("shop".to_owned(), "gate.shopping".to_owned())]);
        assert_eq!(GameFile::from_json(&game.to_json()).unwrap(), game);
        let code = |more: &str| with(more).unwrap_err().code;
        assert_eq!(code(r#""keep": ["best"]"#), "bad_keep");
        assert_eq!(code(r#""keep": {"my.best": "gate.best"}"#), "bad_name");
        // A name is one the player's file's reader takes with what stands before it.
        assert!(with(&format!(r#""keep": {{"{}": "gate.best"}}"#, "n".repeat(MAX_KEEP_NAME))).is_ok());
        assert_eq!(code(&format!(r#""keep": {{"{}": "gate.best"}}"#, "n".repeat(MAX_KEEP_NAME + 1))), "bad_name");
        for key in ["best", "setting.volume", "game.score", "run.ticks", "gate."] {
            assert_eq!(code(&format!(r#""keep": {{"best": "{key}"}}"#)), "bad_key", "{key}");
            assert_eq!(code(&format!(r#""show": {{"shop": "{key}"}}"#)), "bad_key", "{key}");
        }
        assert_eq!(code(r#""show": {"ui/shop": "gate.shopping"}"#), "bad_name");
        let many: Vec<String> = (0..=MAX_KEEP).map(|i| format!(r#""k{i}": "gate.k{i}""#)).collect();
        assert_eq!(code(&format!(r#""keep": {{{}}}"#, many.join(","))), "bad_keep");
    }

    const FULL: &str = r#"{
        "format": "specula.game.v1",
        "title": "Hold the gate",
        "tick_hz": 60,
        "modules": ["modules/gate"],
        "start": "title",
        "screens": { "title": "title", "pause": "pause", "won": "won", "lost": "lost", "hud": "hud" },
        "locale": "en",
        "settings": [
            { "name": "volume", "kind": "int", "min": 0, "max": 10, "default": 7, "is": "volume" },
            { "name": "language", "kind": "locale" },
            { "name": "difficulty", "kind": "choice", "options": ["easy", "normal", "hard"], "default": "normal", "sim": true }
        ],
        "outcome": {
            "lost": { "key": "gate.hp", "lte": 0 },
            "won": { "all": [{ "key": "gate.wave", "gte": 5 }, { "key": "gate.runners", "eq": 0 }] },
            "score": "gate.score"
        }
    }"#;

    #[test]
    fn the_smallest_game_file_is_all_defaults() {
        let g = GameFile::parse(r#"{"format": "specula.game.v1"}"#).unwrap();
        assert_eq!(g, GameFile::default());
        assert_eq!((g.tick_hz, g.start, g.locale.as_str()), (30, Start::Title, "en"));
        assert_eq!(g.screens.parts().map(|(_, name)| name), ["title", "pause", "won", "lost", "hud"]);
    }

    #[test]
    fn a_view_and_a_report_are_read_and_written_back() {
        let g = GameFile::parse(r#"{"format": "specula.game.v1", "title": "Hold", "view": {"center": [10, 6], "height": 12}, "report": {"tick": "run.ticks", "walkers": "gate.walkers"}}"#).unwrap();
        assert_eq!(g.view, Some(View::Flat { center: [10.0, 6.0], height: 12.0 }));
        assert_eq!(g.report, vec![("tick".to_owned(), "run.ticks".to_owned()), ("walkers".to_owned(), "gate.walkers".to_owned())], "in the file's order");
        assert_eq!(GameFile::from_json(&g.to_json()).unwrap(), g);
        let eye = GameFile::parse(r#"{"format": "specula.game.v1", "title": "Hold", "view": {"eye": [0, 5, 10], "target": [0, 0, 0]}}"#).unwrap();
        assert_eq!(eye.view, Some(View::Eye { eye: [0.0, 5.0, 10.0], target: [0.0; 3], fov_y_deg: 50.0 }));
        assert_eq!(GameFile::from_json(&eye.to_json()).unwrap(), eye);
        let bad = |body: &str| GameFile::parse(&format!(r#"{{"format": "specula.game.v1", {body}}}"#)).unwrap_err().code;
        assert_eq!(bad(r#""view": {"center": [1], "height": 2}"#), "bad_view");
        assert_eq!(bad(r#""view": {"center": [1, 2], "height": 0}"#), "bad_view");
        assert_eq!(bad(r#""view": {"center": [1, 2], "height": 3, "zoom": 2}"#), "unknown_key");
        assert_eq!(bad(r#""view": {"eye": [1, 2, 3], "target": [1, 2, 3]}"#), "bad_view");
        assert_eq!(bad(r#""report": ["tick"]"#), "bad_report");
        let words = GameFile::parse(r#"{"format": "specula.game.v1", "title": "Hold", "inputs": {"menu_down": "down", "fire": "up+shoot"}}"#).unwrap();
        assert_eq!(words.inputs, vec![("menu_down".to_owned(), "down".to_owned()), ("fire".to_owned(), "up+shoot".to_owned())]);
        assert_eq!(GameFile::from_json(&words.to_json()).unwrap(), words);
        assert_eq!(bad(r#""inputs": ["down"]"#), "bad_inputs");
        assert_eq!(bad(r#""inputs": {"a+b": "down"}"#), "bad_name");
        assert_eq!(bad(r#""inputs": {"go": 5}"#), "bad_input");
        assert_eq!(bad(r#""report": {"tick": 5}"#), "bad_key");
        assert_eq!(bad(r#""report": {"a b": "run.ticks"}"#), "bad_name");
    }

    #[test]
    fn a_whole_game_file_is_read_and_written_back() {
        let g = GameFile::parse(FULL).unwrap();
        assert_eq!((g.title.as_str(), g.tick_hz, g.modules.as_slice()), ("Hold the gate", 60, ["modules/gate".to_owned()].as_slice()));
        assert_eq!(g.settings.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["volume", "language", "difficulty"]);
        assert_eq!(g.outcome.keys(), ["gate.hp", "gate.wave", "gate.runners", "gate.score"]);
        assert_eq!(GameFile::from_json(&g.to_json()).unwrap(), g, "what it writes, it reads as the same game");
    }

    #[test]
    fn a_run_ends_when_its_condition_holds() {
        let g = GameFile::parse(FULL).unwrap();
        let at = |hp: f64, wave: f64, runners: f64| {
            move |key: &str| match key {
                "gate.hp" => Some(Val::Num(hp)),
                "gate.wave" => Some(Val::Num(wave)),
                "gate.runners" => Some(Val::Num(runners)),
                _ => None,
            }
        };
        assert_eq!(g.outcome.after_tick(&at(3.0, 2.0, 4.0)), Ended::Running);
        assert_eq!(g.outcome.after_tick(&at(0.0, 2.0, 4.0)), Ended::Lost);
        assert_eq!(g.outcome.after_tick(&at(1.0, 5.0, 0.0)), Ended::Won);
        assert_eq!(g.outcome.after_tick(&at(0.0, 5.0, 0.0)), Ended::Lost, "lost and won on one tick is lost");
        // Nothing counted yet: the run is under way.
        assert_eq!(g.outcome.after_tick(&|_: &str| None), Ended::Running);
        assert_eq!(GameFile::default().outcome.after_tick(&at(0.0, 9.0, 0.0)), Ended::Running, "a game that declares no end has none");
        assert_eq!((Ended::named("won"), Ended::named("draw")), (Some(Ended::Won), None));
    }

    #[test]
    fn what_is_wrong_is_said_with_where() {
        let p = |body: &str| GameFile::parse(&format!(r#"{{"format": "specula.game.v1", {body}}}"#)).unwrap_err();
        let at = |body: &str| {
            let p = p(body);
            (p.at, p.code)
        };
        assert_eq!(GameFile::parse("{}").unwrap_err().code, "bad_format");
        assert_eq!(GameFile::parse("nope").unwrap_err().code, "not_json");
        assert_eq!(GameFile::parse("[]").unwrap_err().code, "not_an_object");
        assert_eq!(at(r#""tick_rate": 60"#), ("tick_rate".to_owned(), "unknown_key"), "a misspelt key is not a default silently kept");
        assert!(p(r#""tick_rate": 60"#).message.contains("tick_hz"));
        assert_eq!(at(r#""tick_hz": 0"#).1, "bad_tick_rate");
        assert_eq!(at(r#""tick_hz": 1000"#).1, "bad_tick_rate");
        assert_eq!(at(r#""tick_hz": 59.94"#).1, "bad_tick_rate");
        assert_eq!(at(r#""title": """#).1, "bad_title");
        assert_eq!(at(r#""modules": ["../outside"]"#), ("modules[0]".to_owned(), "bad_module"));
        assert_eq!(at(r#""modules": ["C:/x"]"#).1, "bad_module");
        assert_eq!(at(r#""modules": ["modules\\gate"]"#).1, "bad_module");
        assert_eq!(at(r#""modules": ["m/a", "m/a"]"#), ("modules[1]".to_owned(), "bad_module"));
        assert_eq!(at(r#""start": "menu""#).1, "bad_start");
        assert_eq!(at(r#""screens": {"title": "Main Menu"}"#), ("screens.title".to_owned(), "bad_screen"));
        assert_eq!(at(r#""screens": {"credits": "credits"}"#), ("screens.credits".to_owned(), "unknown_key"));
        assert_eq!(at(r#""locale": "en_US""#).1, "bad_locale");
        assert_eq!(at(r#""settings": [{"name": "a", "kind": "toggle"}, {"name": "a", "kind": "toggle"}]"#), ("settings[1].name".to_owned(), "bad_name"));
        assert_eq!(at(r#""settings": [{"name": "a", "kind": "locale"}, {"name": "b", "kind": "locale"}]"#).1, "bad_kind");
        assert_eq!(at(r#""settings": [{"name": "a", "kind": "int", "min": 0, "max": 9, "is": "volume"}, {"name": "b", "kind": "int", "min": 0, "max": 9, "is": "volume"}]"#).1, "bad_meaning");
        assert_eq!(at(r#""outcome": {"won": {"key": "a", "over": 1}}"#), ("outcome.won.over".to_owned(), "unknown_matcher"));
        assert_eq!(at(r#""outcome": {"score": 10}"#), ("outcome.score".to_owned(), "bad_key"));
        assert_eq!(at(r#""outcome": {"draw": "a"}"#).1, "unknown_key");
        assert_eq!(GameFile::parse(&format!(r#"{{"format": "specula.game.v1", "title": "{}"}}"#, "x".repeat(MAX_BYTES))).unwrap_err().code, "too_big");
    }
}

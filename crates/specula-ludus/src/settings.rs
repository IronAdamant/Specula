//! What a player may set, and what they chose.
//!
//! A game declares its settings in the game file; the project shell shows them, changes them
//! from a menu's buttons ([`crate::Action`]) and keeps them between runs in the settings file
//! the engine manages for each user. Nothing here reads or writes a file: this is the settings'
//! meaning, and the text of the file.
//!
//! Two settings every game has unless it declares its own: `language` (the string tables the
//! project has) and `volume` (0 to 10).
//!
//! **A file is read forgivingly.** A player's settings file outlives the game's versions: a
//! value that no longer fits its setting is the default again, and a name the game no longer
//! has is kept as it was found (so an older build of the game still finds it), up to
//! [`MAX_EXTRA`].

use std::collections::BTreeMap;

use specula_core::Json;

use crate::cond::Val;
use crate::{name_ok, unknown_keys, Problem};

pub const FORMAT: &str = "specula.settings.v1";
/// Most settings a game declares, options a choice has, and unknown names a file keeps.
pub const MAX_SETTINGS: usize = 64;
pub const MAX_OPTIONS: usize = 32;
pub const MAX_EXTRA: usize = 32;
/// What stands before the name of a value the game keeps for the player, in the settings
/// file: a setting's name has no dot.
pub const KEPT: &str = "kept.";
/// What a setting may stand for in the engine (`"is"`).
pub const MEANINGS: [&str; 3] = ["volume", "ui_scale", "text_size"];

/// What kind of thing a setting is.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingKind {
    /// A whole number from `min` to `max`.
    Int { min: i64, max: i64 },
    /// One of these words.
    Choice { options: Vec<String> },
    /// On or off.
    Toggle,
    /// One of the languages the project has string tables for.
    Locale,
}

/// One setting a game declares.
#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub name: String,
    pub kind: SettingKind,
    /// What it is until the player chooses (for a language: the game file's `locale`).
    pub default: Option<Val>,
    /// What it stands for in the engine, if anything: one of [`MEANINGS`].
    pub is: Option<String>,
    /// The game's rules read it (a difficulty): the modules are told its value at the start of a
    /// run and when it changes, at a tick boundary.
    pub sim: bool,
}

impl Setting {
    /// Reads one setting of a game file's `settings` list.
    pub fn parse(json: &Json, at: &str) -> Result<Setting, Problem> {
        unknown_keys(json, &["name", "kind", "min", "max", "options", "default", "is", "sim"], at)?;
        let text = |key: &str| json.get(key).and_then(Json::as_str);
        let name = text("name").filter(|n| name_ok(n, 32)).ok_or_else(|| Problem::new(&format!("{at}.name"), "bad_name", "a setting's name is a lowercase letter, then lowercase letters, digits and _ (at most 32)"))?;
        let only = |keys: &[&str], kind: &str| match keys.iter().find(|k| json.get(k).is_some()) {
            Some(key) => Err(Problem::new(&format!("{at}.{key}"), "not_this_kind", format!("a setting of kind \"{kind}\" has no \"{key}\""))),
            None => Ok(()),
        };
        let kind_at = format!("{at}.kind");
        let (kind, default) = match text("kind") {
            Some("int") => {
                only(&["options"], "int")?;
                let whole = |key: &str| json.get(key).and_then(Json::as_i64).filter(|n| n.abs() <= 1_000_000).ok_or_else(|| Problem::new(&format!("{at}.{key}"), "bad_number", format!("\"{key}\" is a whole number, at most a million either way")));
                let (min, max) = (whole("min")?, whole("max")?);
                if min >= max {
                    return Err(Problem::new(&format!("{at}.max"), "bad_range", format!("\"max\" ({max}) must be more than \"min\" ({min})")));
                }
                let default = match json.get("default") {
                    None => min,
                    Some(_) => whole("default")?,
                };
                if !(min..=max).contains(&default) {
                    return Err(Problem::new(&format!("{at}.default"), "bad_default", format!("the default ({default}) is not from {min} to {max}")));
                }
                (SettingKind::Int { min, max }, Some(Val::Num(default as f64)))
            }
            Some("choice") => {
                only(&["min", "max"], "choice")?;
                let here = format!("{at}.options");
                let list = json.get("options").and_then(Json::as_array).filter(|o| (2..=MAX_OPTIONS).contains(&o.len())).ok_or_else(|| Problem::new(&here, "bad_options", format!("\"options\" is a list of 2 to {MAX_OPTIONS} words")))?;
                let mut options: Vec<String> = Vec::new();
                for (i, o) in list.iter().enumerate() {
                    let word = o.as_str().filter(|w| name_ok(w, 32)).ok_or_else(|| Problem::new(&format!("{here}[{i}]"), "bad_option", "an option is a word: a lowercase letter, then lowercase letters, digits and _ (its text for the player comes from the string tables)"))?;
                    if options.iter().any(|have| have == word) {
                        return Err(Problem::new(&format!("{here}[{i}]"), "bad_option", format!("\"{word}\" is there twice")));
                    }
                    options.push(word.to_owned());
                }
                let default = match json.get("default") {
                    None => options[0].clone(),
                    Some(d) => d.as_str().filter(|d| options.iter().any(|o| o == d)).ok_or_else(|| Problem::new(&format!("{at}.default"), "bad_default", format!("the default is one of the options: {}", options.join(", "))))?.to_owned(),
                };
                (SettingKind::Choice { options }, Some(Val::Text(default)))
            }
            Some("toggle") => {
                only(&["min", "max", "options"], "toggle")?;
                let default = match json.get("default") {
                    None => false,
                    Some(d) => d.as_bool().ok_or_else(|| Problem::new(&format!("{at}.default"), "bad_default", "a toggle's default is true or false"))?,
                };
                (SettingKind::Toggle, Some(Val::Bool(default)))
            }
            Some("locale") => {
                only(&["min", "max", "options", "default", "is"], "locale")?;
                (SettingKind::Locale, None)
            }
            _ => return Err(Problem::new(&kind_at, "bad_kind", "a setting's \"kind\" is one of: int, choice, toggle, locale")),
        };
        let is = match json.get("is") {
            None => None,
            Some(m) => {
                let word = m.as_str().filter(|w| MEANINGS.contains(w)).ok_or_else(|| Problem::new(&format!("{at}.is"), "bad_meaning", format!("\"is\" is one of: {}", MEANINGS.join(", "))))?;
                if !matches!(kind, SettingKind::Int { .. }) {
                    return Err(Problem::new(&format!("{at}.is"), "bad_meaning", format!("\"{word}\" is a number: the setting's kind must be \"int\"")));
                }
                Some(word.to_owned())
            }
        };
        let sim = match json.get("sim") {
            None => false,
            Some(s) => s.as_bool().ok_or_else(|| Problem::new(&format!("{at}.sim"), "bad_flag", "\"sim\" is true or false"))?,
        };
        Ok(Setting { name: name.to_owned(), kind, default, is, sim })
    }

    /// The two settings every game has unless it declares its own.
    pub fn language() -> Setting {
        Setting { name: "language".to_owned(), kind: SettingKind::Locale, default: None, is: None, sim: false }
    }

    pub fn volume() -> Setting {
        Setting { name: "volume".to_owned(), kind: SettingKind::Int { min: 0, max: 10 }, default: Some(Val::Num(8.0)), is: Some("volume".to_owned()), sim: false }
    }
}

/// The player's own choices, as their settings file holds them: each name with the value the
/// player gave it, whatever the game declares now.
///
/// It is kept apart from [`Settings`] (what the game uses this session) because the two differ
/// in whose they are. A setting the player never touched is not in the file, so a default the
/// game file's author changes is not written back as the player's choice. A choice the game
/// no longer takes (its options changed, its language's table is gone) stays in the file as
/// the player made it: it is not used while it does not fit, and it is the player's again
/// when it does. Only the player's own change replaces it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kept {
    values: Vec<(String, Json)>,
}

impl Kept {
    /// The choices in a settings file's text (none, when the text is not a settings file).
    /// At most [`MAX_SETTINGS`] and [`MAX_EXTRA`] names together, each of at most 64 characters.
    pub fn read_text(text: &str) -> Kept {
        let mut kept = Kept::default();
        let Ok(json) = Json::parse(text) else { return kept };
        if json.get("format").and_then(Json::as_str) != Some(FORMAT) {
            return kept;
        }
        for (name, found) in json.get("values").and_then(Json::as_object).unwrap_or_default() {
            if kept.values.len() < MAX_SETTINGS + MAX_EXTRA && name.chars().count() <= 64 {
                kept.values.push((name.clone(), found.clone()));
            }
        }
        kept
    }

    /// The player gave `name` this value.
    pub fn set(&mut self, name: &str, value: &Val) {
        if let Some(at) = self.values.iter().position(|(k, _)| k == name) {
            self.values[at].1 = value.to_json();
        } else if self.values.len() < MAX_SETTINGS + MAX_EXTRA && name.chars().count() <= 64 {
            self.values.push((name.to_owned(), value.to_json()));
        }
    }

    /// The game keeps `value` for the player under `name` ([`KEPT`] before it in the file,
    /// so that no setting can have its name). Whether anything changed.
    pub fn keep(&mut self, name: &str, value: Json) -> bool {
        let name = format!("{KEPT}{name}");
        if let Some(at) = self.values.iter().position(|(k, _)| *k == name) {
            if self.values[at].1 == value {
                return false;
            }
            self.values[at].1 = value;
            true
        } else if self.values.len() < MAX_SETTINGS + MAX_EXTRA {
            self.values.push((name, value));
            true
        } else {
            false
        }
    }

    /// What the game kept under `name` ([`Kept::keep`]).
    pub fn kept(&self, name: &str) -> Option<&Json> {
        self.values.iter().find(|(k, _)| k.strip_prefix(KEPT) == Some(name)).map(|(_, v)| v)
    }

    /// What the player chose for `name`, as the file holds it.
    pub fn get(&self, name: &str) -> Option<&Json> {
        self.values.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    /// The choices as the settings file's text.
    pub fn to_text(&self) -> String {
        let mut text = Json::obj([("format", Json::Str(FORMAT.to_owned())), ("values", Json::Obj(self.values.clone()))]).to_pretty();
        text.push('\n');
        text
    }
}

/// A game's settings and the values they have.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    defs: Vec<Setting>,
    /// The languages a `locale` setting may be, the default first.
    locales: Vec<String>,
    values: BTreeMap<String, Val>,
    /// Names a file held that this game does not declare, kept as found.
    extra: Vec<(String, Json)>,
}

impl Settings {
    /// The settings `declared` in the game file, with the two every game has added when it
    /// declares neither a `locale` setting nor one that `is` the volume. `locales`: the
    /// languages there are string tables for, the game's own first (never empty: a game with no
    /// table has its one language all the same).
    pub fn new(declared: &[Setting], locales: &[String]) -> Settings {
        let mut defs = declared.to_vec();
        if !defs.iter().any(|d| d.kind == SettingKind::Locale) && !defs.iter().any(|d| d.name == "language") {
            defs.push(Setting::language());
        }
        if !defs.iter().any(|d| d.is.as_deref() == Some("volume")) && !defs.iter().any(|d| d.name == "volume") {
            defs.push(Setting::volume());
        }
        let locales = if locales.is_empty() { vec!["en".to_owned()] } else { locales.to_vec() };
        let mut s = Settings { defs, locales, values: BTreeMap::new(), extra: Vec::new() };
        s.reset();
        s
    }

    /// Every setting back to its default.
    pub fn reset(&mut self) {
        self.values = self.defs.iter().map(|d| (d.name.clone(), self.default_of(d))).collect();
    }

    fn default_of(&self, def: &Setting) -> Val {
        def.default.clone().unwrap_or_else(|| Val::Text(self.locales[0].clone()))
    }

    pub fn defs(&self) -> &[Setting] {
        &self.defs
    }

    pub fn def(&self, name: &str) -> Option<&Setting> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn locales(&self) -> &[String] {
        &self.locales
    }

    /// A setting's value now.
    pub fn get(&self, name: &str) -> Option<&Val> {
        self.values.get(name)
    }

    /// The value of the setting that stands for `meaning` (`"volume"`), as a fraction of its
    /// range, 0 to 1.
    pub fn fraction(&self, meaning: &str) -> Option<f64> {
        let def = self.defs.iter().find(|d| d.is.as_deref() == Some(meaning))?;
        match (&def.kind, self.values.get(&def.name)) {
            (SettingKind::Int { min, max }, Some(Val::Num(n))) => Some((n - *min as f64) / (*max - *min) as f64),
            _ => None,
        }
    }

    /// The chosen language.
    pub fn locale(&self) -> &str {
        let chosen = self.defs.iter().find(|d| d.kind == SettingKind::Locale).and_then(|d| self.values.get(&d.name));
        match chosen {
            Some(Val::Text(l)) => l,
            _ => &self.locales[0],
        }
    }

    /// Whether `value` is one `def` may have.
    fn fits(&self, def: &Setting, value: &Val) -> bool {
        match (&def.kind, value) {
            (SettingKind::Int { min, max }, Val::Num(n)) => specula_core::dmath::fract_f64(*n) == 0.0 && *n >= *min as f64 && *n <= *max as f64,
            (SettingKind::Choice { options }, Val::Text(t)) => options.iter().any(|o| o == t),
            (SettingKind::Toggle, Val::Bool(_)) => true,
            (SettingKind::Locale, Val::Text(t)) => self.locales.iter().any(|l| l == t),
            _ => false,
        }
    }

    /// Gives a setting a value. `Ok(true)`: it changed; `Ok(false)`: it had that value.
    pub fn set(&mut self, name: &str, value: Val) -> Result<bool, String> {
        let def = self.def(name).ok_or_else(|| self.no_such(name))?.clone();
        if !self.fits(&def, &value) {
            return Err(format!("\"{name}\" takes {}", self.takes(&def)));
        }
        let changed = self.values.get(name) != Some(&value);
        self.values.insert(name.to_owned(), value);
        Ok(changed)
    }

    /// Gives a setting a value written as text, as a button's `set:NAME:VALUE` does.
    pub fn set_text(&mut self, name: &str, text: &str) -> Result<bool, String> {
        let def = self.def(name).ok_or_else(|| self.no_such(name))?;
        let value = match def.kind {
            SettingKind::Int { .. } => text.parse::<i64>().map(|n| Val::Num(n as f64)).map_err(|_| format!("\"{name}\" takes {}", self.takes(def)))?,
            SettingKind::Toggle => match text {
                "on" | "true" => Val::Bool(true),
                "off" | "false" => Val::Bool(false),
                _ => return Err(format!("\"{name}\" takes {}", self.takes(def))),
            },
            SettingKind::Choice { .. } | SettingKind::Locale => Val::Text(text.to_owned()),
        };
        self.set(name, value)
    }

    /// Moves a setting one step up or down. It stops at its ends: `Ok(false)` there.
    pub fn step(&mut self, name: &str, up: bool) -> Result<bool, String> {
        self.moved(name, up, false)
    }

    /// Moves a setting to its next value, and round to the first after the last.
    pub fn cycle(&mut self, name: &str) -> Result<bool, String> {
        self.moved(name, true, true)
    }

    fn moved(&mut self, name: &str, up: bool, round: bool) -> Result<bool, String> {
        let def = self.def(name).ok_or_else(|| self.no_such(name))?.clone();
        let now = self.values.get(name).cloned().unwrap_or_else(|| self.default_of(&def));
        // The setting's values as a row, and where it is in it.
        let among = |list: &[String], now: &Val| match now {
            Val::Text(t) => list.iter().position(|o| o == t).unwrap_or(0),
            _ => 0,
        };
        let (at, count): (i64, i64) = match (&def.kind, &now) {
            (SettingKind::Int { min, max }, Val::Num(n)) => (*n as i64 - min, max - min + 1),
            (SettingKind::Int { min, max }, _) => (0, max - min + 1),
            (SettingKind::Choice { options }, now) => (among(options, now) as i64, options.len() as i64),
            (SettingKind::Locale, now) => (among(&self.locales, now) as i64, self.locales.len() as i64),
            (SettingKind::Toggle, Val::Bool(b)) => (i64::from(*b), 2),
            (SettingKind::Toggle, _) => (0, 2),
        };
        let next = at + if up { 1 } else { -1 };
        let next = if round { next.rem_euclid(count) } else { next.clamp(0, count - 1) };
        let value = match &def.kind {
            SettingKind::Int { min, .. } => Val::Num((min + next) as f64),
            SettingKind::Choice { options } => Val::Text(options[next as usize].clone()),
            SettingKind::Locale => Val::Text(self.locales[next as usize].clone()),
            SettingKind::Toggle => Val::Bool(next == 1),
        };
        self.set(name, value)
    }

    fn no_such(&self, name: &str) -> String {
        format!("there is no setting \"{name}\"; there are: {}", self.defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", "))
    }

    /// What a setting takes, in words.
    fn takes(&self, def: &Setting) -> String {
        match &def.kind {
            SettingKind::Int { min, max } => format!("a whole number from {min} to {max}"),
            SettingKind::Choice { options } => format!("one of: {}", options.join(", ")),
            SettingKind::Toggle => "on or off".to_owned(),
            SettingKind::Locale => format!("one of the game's languages: {}", self.locales.join(", ")),
        }
    }

    /// The values as the settings file's text.
    pub fn to_text(&self) -> String {
        let mut values: Vec<(String, Json)> = self.values.iter().map(|(k, v)| (k.clone(), v.to_json())).collect();
        values.extend(self.extra.iter().cloned());
        let mut text = Json::obj([("format", Json::Str(FORMAT.to_owned())), ("values", Json::Obj(values))]).to_pretty();
        text.push('\n');
        text
    }

    /// Takes the values of a settings file's text. Never fails: a file that is not one leaves
    /// every setting at its default, and the answer says what was not taken.
    pub fn read_text(&mut self, text: &str) -> Vec<String> {
        self.reset();
        self.extra.clear();
        let mut notes = Vec::new();
        let json = match Json::parse(text) {
            Ok(j) => j,
            Err(e) => return vec![format!("the settings file is not JSON ({e}): the defaults are used")],
        };
        if json.get("format").and_then(Json::as_str) != Some(FORMAT) {
            return vec![format!("the settings file is not \"{FORMAT}\": the defaults are used")];
        }
        for (name, found) in json.get("values").and_then(Json::as_object).unwrap_or_default() {
            match (self.def(name).cloned(), Val::of(found)) {
                (Some(def), Some(value)) if self.fits(&def, &value) => {
                    self.values.insert(name.clone(), value);
                }
                (Some(_), _) => notes.push(format!("\"{name}\" held a value it no longer takes: it is the default again")),
                (None, _) if self.extra.len() < MAX_EXTRA && name.chars().count() <= 64 => self.extra.push((name.clone(), found.clone())),
                (None, _) => notes.push(format!("\"{name}\" is not a setting of this game and was not kept")),
            }
        }
        notes
    }

    /// The settings the game's rules read, with their values.
    pub fn sim_values(&self) -> Vec<(&str, &Val)> {
        self.defs.iter().filter(|d| d.sim).filter_map(|d| self.values.get(&d.name).map(|v| (d.name.as_str(), v))).collect()
    }

    /// Whether the game's rules read this setting.
    pub fn is_sim(&self, name: &str) -> bool {
        self.def(name).is_some_and(|d| d.sim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a game keeps stands under its own names in the player's file, beside the settings
    /// and apart from them.
    #[test]
    fn what_a_game_keeps_is_beside_the_settings() {
        let mut kept = Kept::read_text(r#"{"format": "specula.settings.v1", "values": {"volume": 3}}"#);
        assert_eq!(kept.kept("best"), None);
        assert!(kept.keep("best", Json::Num(40.0)));
        assert!(!kept.keep("best", Json::Num(40.0)), "the same again changes nothing");
        assert!(kept.keep("best", Json::Num(50.0)));
        assert!(kept.keep("volume", Json::Num(9.0)), "a kept value named as a setting is another name in the file");
        assert_eq!((kept.kept("best"), kept.kept("volume"), kept.get("volume")), (Some(&Json::Num(50.0)), Some(&Json::Num(9.0)), Some(&Json::Num(3.0))));
        let again = Kept::read_text(&kept.to_text());
        assert_eq!(again.kept("best"), Some(&Json::Num(50.0)));
    }


    fn def(text: &str) -> Result<Setting, Problem> {
        Setting::parse(&Json::parse(text).expect("the test's JSON"), "settings[0]")
    }

    fn game() -> Settings {
        let defs = [
            def(r#"{"name": "volume", "kind": "int", "min": 0, "max": 10, "default": 7, "is": "volume"}"#).unwrap(),
            def(r#"{"name": "language", "kind": "locale"}"#).unwrap(),
            def(r#"{"name": "difficulty", "kind": "choice", "options": ["easy", "normal", "hard"], "default": "normal", "sim": true}"#).unwrap(),
            def(r#"{"name": "hints", "kind": "toggle", "default": true}"#).unwrap(),
        ];
        Settings::new(&defs, &["en".to_owned(), "es".to_owned()])
    }

    #[test]
    fn a_setting_is_read_with_its_default() {
        let d = def(r#"{"name": "speed", "kind": "int", "min": 1, "max": 4}"#).unwrap();
        assert_eq!((d.kind, d.default, d.is, d.sim), (SettingKind::Int { min: 1, max: 4 }, Some(Val::Num(1.0)), None, false));
        let d = def(r#"{"name": "mode", "kind": "choice", "options": ["a", "b"]}"#).unwrap();
        assert_eq!(d.default, Some(Val::Text("a".into())));
        assert_eq!(def(r#"{"name": "hints", "kind": "toggle"}"#).unwrap().default, Some(Val::Bool(false)));
        assert_eq!(def(r#"{"name": "language", "kind": "locale"}"#).unwrap().default, None, "the project's own language");
    }

    #[test]
    fn a_setting_that_is_wrong_says_what_to_write() {
        let p = |text: &str| def(text).map(|_| ()).unwrap_err();
        assert_eq!(p(r#"{"name": "Volume", "kind": "int", "min": 0, "max": 1}"#).code, "bad_name");
        assert_eq!(p(r#"{"name": "v", "kind": "slider"}"#).code, "bad_kind");
        assert_eq!(p(r#"{"name": "v", "kind": "int", "min": 5, "max": 5}"#).code, "bad_range");
        assert_eq!(p(r#"{"name": "v", "kind": "int", "min": 0, "max": 5, "default": 9}"#).code, "bad_default");
        assert_eq!(p(r#"{"name": "v", "kind": "int", "min": 0, "max": 1.5}"#).code, "bad_number");
        assert_eq!(p(r#"{"name": "v", "kind": "int", "min": 0, "max": 5, "options": []}"#).code, "not_this_kind");
        assert_eq!(p(r#"{"name": "v", "kind": "choice", "options": ["a"]}"#).code, "bad_options");
        assert_eq!(p(r#"{"name": "v", "kind": "choice", "options": ["a", "a"]}"#).code, "bad_option");
        assert_eq!(p(r#"{"name": "v", "kind": "choice", "options": ["a", "b"], "default": "c"}"#).code, "bad_default");
        assert_eq!(p(r#"{"name": "v", "kind": "toggle", "default": "yes"}"#).code, "bad_default");
        assert_eq!(p(r#"{"name": "v", "kind": "toggle", "is": "volume"}"#).code, "bad_meaning");
        assert_eq!(p(r#"{"name": "v", "kind": "int", "min": 0, "max": 5, "is": "brightness"}"#).code, "bad_meaning");
        assert_eq!(p(r#"{"name": "v", "kind": "locale", "default": "en"}"#).code, "not_this_kind");
        let unknown = p(r#"{"name": "v", "kind": "toggle", "label": "x"}"#);
        assert_eq!((unknown.at.as_str(), unknown.code), ("settings[0].label", "unknown_key"));
    }

    #[test]
    fn every_game_has_a_language_and_a_volume() {
        let s = Settings::new(&[], &[]);
        assert_eq!(s.defs().iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["language", "volume"]);
        assert_eq!((s.locale(), s.get("volume"), s.fraction("volume")), ("en", Some(&Val::Num(8.0)), Some(0.8)));
        // A game that declares its own has no second one.
        assert_eq!(game().defs().len(), 4);
        assert_eq!(game().fraction("volume"), Some(0.7));
    }

    #[test]
    fn buttons_change_settings_within_what_they_take() {
        let mut s = game();
        assert_eq!(s.step("volume", true), Ok(true));
        assert_eq!(s.get("volume"), Some(&Val::Num(8.0)));
        assert_eq!((s.set_text("volume", "10"), s.step("volume", true)), (Ok(true), Ok(false)), "it stops at its end");
        assert_eq!(s.set_text("volume", "0").and_then(|_| s.step("volume", false)), Ok(false));
        assert!(s.set_text("volume", "11").unwrap_err().contains("from 0 to 10") && s.set_text("volume", "loud").is_err());
        // A choice steps and stops; a cycle goes round.
        assert_eq!((s.step("difficulty", true), s.get("difficulty")), (Ok(true), Some(&Val::Text("hard".into()))));
        assert_eq!(s.step("difficulty", true), Ok(false));
        assert_eq!((s.cycle("difficulty"), s.get("difficulty")), (Ok(true), Some(&Val::Text("easy".into()))));
        assert!(s.set_text("difficulty", "brutal").unwrap_err().contains("easy, normal, hard"));
        // The language goes through the project's own.
        assert_eq!(s.locale(), "en");
        assert_eq!((s.cycle("language"), s.locale()), (Ok(true), "es"));
        assert_eq!((s.cycle("language"), s.locale()), (Ok(true), "en"));
        assert!(s.set_text("language", "fr").unwrap_err().contains("en, es"));
        // A toggle.
        assert_eq!((s.cycle("hints"), s.get("hints")), (Ok(true), Some(&Val::Bool(false))));
        assert_eq!((s.set_text("hints", "on"), s.set_text("hints", "on")), (Ok(true), Ok(false)));
        assert!(s.set_text("nothing", "1").unwrap_err().contains("volume, language, difficulty, hints"));
        // What the rules read.
        assert_eq!(s.sim_values(), [("difficulty", &Val::Text("easy".into()))]);
        assert!(s.is_sim("difficulty") && !s.is_sim("volume"));
    }

    #[test]
    fn the_file_comes_back_as_it_was_written() {
        let mut s = game();
        s.set_text("volume", "3").unwrap();
        s.set_text("language", "es").unwrap();
        s.set_text("difficulty", "hard").unwrap();
        let text = s.to_text();
        assert!(text.contains(FORMAT) && text.ends_with('\n'));
        let mut back = game();
        assert_eq!(back.read_text(&text), Vec::<String>::new());
        assert_eq!(back, s);
    }

    /// A player's file outlives the game's versions.
    #[test]
    fn a_file_is_read_forgivingly() {
        let mut s = game();
        let notes = s.read_text(r#"{"format": "specula.settings.v1", "values": {"volume": 99, "difficulty": "brutal", "language": "es", "hints": "yes", "old_setting": [1, 2]}}"#);
        assert_eq!(notes.len(), 3, "{notes:?}");
        assert_eq!((s.get("volume"), s.get("difficulty"), s.locale(), s.get("hints")), (Some(&Val::Num(7.0)), Some(&Val::Text("normal".into())), "es", Some(&Val::Bool(true))));
        assert!(s.to_text().contains("old_setting"), "a name this game does not have is kept as found");
        for not_one in ["", "not json", "[]", r#"{"format": "other.v1", "values": {"volume": 1}}"#] {
            assert_eq!(s.read_text(not_one).len(), 1, "{not_one}");
            assert_eq!(s, game(), "the defaults");
        }
        // A language whose table is gone is the game's own again.
        let mut one = Settings::new(&[], &["en".to_owned()]);
        assert_eq!(one.read_text(r#"{"format": "specula.settings.v1", "values": {"language": "es"}}"#).len(), 1);
        assert_eq!(one.locale(), "en");
    }
}

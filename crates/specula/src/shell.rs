//! The project shell: the game host a module-built game gets from the engine
//! (docs/design/FIRST_HOUR.md, slice 4).
//!
//! A hosted game's own executable (Sepulcra's) brings its title screen, its pause menu, its
//! settings and its words. A game made of modules and data had none of them: its builder wrote
//! menus, string tables and a settings file by hand, in the module or beside it (M5's measured
//! comparison). This is that shell, written once: a [`Game`] the engine hosts when the project
//! it opens has a `game.json` and no executable of its own.
//!
//! - **Screens** are the project's data (`assets/ui/*.json`), drawn and driven by `specula-ui`;
//!   the `ui_*` tools work on them as on any hosted game's.
//! - **What a screen shows is what the game keeps**: `<module>.<key>` is a value a module set
//!   with `Cx::set_state`; `setting.<name>` a setting; `game.*` the run's own state.
//! - **What a press does** is a word ([`specula_ludus::Action`]): the run's own words are done
//!   here; any other reaches the modules as an event at the next tick boundary.
//! - **The run** (title, running, paused, won, lost) is [`specula_ludus::Run`]: this file is its
//!   plumbing to the window, the clock, the modules and the disk, and decides nothing itself.
//!
//! Reading is one way: nothing of the interface reaches the simulation except the events the
//! run hands out.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use specula_core::Json;
use specula_input::{ActionMap, Input};
use specula_lingua::{Catalog, Table};
use specula_ludus::game::Ended;
use specula_ludus::{Action, Effect, GameFile, Phase, Run, Settings, Val};
use specula_platform::{GamepadState, MAX_GAMEPADS};
use specula_sdk::Value;
use specula_ui::gpu::GpuOverlay;
use specula_ui::{Display, FontSet, StateMap, Strings, Theme, ToastKind, Ui, UiEvent, UiEvents};

use crate::game::{Ctx, Frame, Game, GameLocale, GameRequest, GameUi, ModuleStates, Overlay, SettingsStore, UiPick};

/// The files a new project is given, and the shell's own copies for a project that lacks one:
/// path below the project (with `/`), text.
pub const BUILTIN: &[(&str, &str)] = &[
    ("assets/ui/theme.json", include_str!("shell/new/assets/ui/theme.json")),
    ("assets/ui/title.json", include_str!("shell/new/assets/ui/title.json")),
    ("assets/ui/pause.json", include_str!("shell/new/assets/ui/pause.json")),
    ("assets/ui/settings.json", include_str!("shell/new/assets/ui/settings.json")),
    ("assets/ui/won.json", include_str!("shell/new/assets/ui/won.json")),
    ("assets/ui/lost.json", include_str!("shell/new/assets/ui/lost.json")),
    ("assets/ui/hud.json", include_str!("shell/new/assets/ui/hud.json")),
    ("assets/strings/en.json", include_str!("shell/new/assets/strings/en.json")),
];

fn builtin(path: &str) -> &'static str {
    BUILTIN.iter().find(|(p, _)| *p == path).map_or("", |(_, text)| text)
}

/// Most screens and string tables read from a project, and the largest of each (a screen or a
/// table is a few KiB).
const MAX_SCREENS: usize = 64;
const MAX_TABLES: usize = 32;
const MAX_FILE: u64 = 512 << 10;
/// Most module values a screen is shown, the longest text among them, and most rows of a list.
const MAX_SHOWN: usize = 512;
const MAX_SHOWN_TEXT: usize = 256;
const MAX_ROWS: usize = 64;
/// Most events waiting for the modules (a held button at a title screen must not grow without
/// bound while the clock stands still).
const MAX_EVENTS: usize = 256;
/// The interface's own input: the menu actions `specula-ui` reads.
const PAUSE: &str = "ui_back";

/// What a project's shell is made from, as text: read from the project's folder at start
/// ([`ShellFiles::read`]) or given whole (tests).
#[derive(Clone, Debug, Default)]
pub struct ShellFiles {
    /// `game.json`.
    pub game: String,
    /// `assets/ui/theme.json`, or the built-in one.
    pub theme: String,
    /// Screens by file stem, in name order.
    pub screens: Vec<(String, String)>,
    /// String tables by file stem.
    pub tables: Vec<(String, String)>,
    /// What could not be read.
    pub problems: Vec<String>,
}

impl ShellFiles {
    /// The built-in files, with `game` as the game file: a project that has only a `game.json`.
    pub fn builtin(game: &str) -> ShellFiles {
        let stem = |path: &str| path.rsplit('/').next().unwrap_or(path).trim_end_matches(".json").to_owned();
        let of = |folder: &str| BUILTIN.iter().filter(|(p, _)| p.starts_with(folder) && !p.ends_with("theme.json")).map(|(p, text)| (stem(p), (*text).to_owned())).collect();
        ShellFiles { game: game.to_owned(), theme: builtin("assets/ui/theme.json").to_owned(), screens: of("assets/ui/"), tables: of("assets/strings/"), problems: Vec::new() }
    }

    /// Reads a project's shell files: its game file, and under its assets the theme, the
    /// screens and the string tables. A part the project lacks is the built-in one (the five
    /// screens the game file's parts name by default, the theme, the English table when the
    /// project has no table at all). Small bounded reads: at start, and when the files change.
    pub fn read(project: &Path, assets: &Path) -> ShellFiles {
        let mut files = ShellFiles::default();
        let text = |path: PathBuf, what: &str, problems: &mut Vec<String>| -> Option<String> {
            let len = std::fs::metadata(&path).ok().filter(|m| m.is_file())?.len();
            if len > MAX_FILE {
                problems.push(format!("{what} is {len} bytes: at most {MAX_FILE} are read"));
                return None;
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => Some(text),
                Err(e) => {
                    problems.push(format!("{what} could not be read ({e})"));
                    None
                }
            }
        };
        let mut problems = Vec::new();
        files.game = text(project.join(specula_ludus::game::FILE), specula_ludus::game::FILE, &mut problems).unwrap_or_default();
        files.theme = text(assets.join("ui").join("theme.json"), "assets/ui/theme.json", &mut problems).unwrap_or_else(|| builtin("assets/ui/theme.json").to_owned());
        let folder = |name: &str, most: usize, problems: &mut Vec<String>| -> Vec<(String, String)> {
            let mut stems: Vec<String> = std::fs::read_dir(assets.join(name))
                .map(|entries| entries.flatten().filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).map(str::to_owned)).collect())
                .unwrap_or_default();
            stems.retain(|s| !(name == "ui" && s == "theme"));
            stems.sort();
            if stems.len() > most {
                problems.push(format!("assets/{name} holds {} files: the first {most} are read", stems.len()));
                stems.truncate(most);
            }
            stems.into_iter().filter_map(|stem| text(assets.join(name).join(format!("{stem}.json")), &format!("assets/{name}/{stem}.json"), problems).map(|t| (stem, t))).collect()
        };
        files.screens = folder("ui", MAX_SCREENS, &mut problems);
        files.tables = folder("strings", MAX_TABLES, &mut problems);
        // What the project lacks: the default screens, and a table when it has none.
        let own = ShellFiles::builtin("");
        for (stem, text) in own.screens {
            if !files.screens.iter().any(|(have, _)| *have == stem) {
                files.screens.push((stem, text));
            }
        }
        if files.tables.is_empty() {
            files.tables = own.tables;
        }
        files.problems = problems;
        files
    }
}

/// A module's value as a screen shows it.
#[derive(Clone, Debug, PartialEq)]
enum Shown {
    Bool(bool),
    Int(i64),
    Num(f64),
    Text(String),
    /// A line of the string tables with its arguments.
    Msg(String, Vec<(String, Shown)>),
    /// Rows of fields.
    Rows(Vec<Vec<(String, Shown)>>),
}

impl Shown {
    /// A module's value as a screen can show it: a scalar, a map with `msg` (a string-table
    /// line and its `args`), or a list of maps of scalars (rows). Anything else is not shown.
    fn of(value: &Value) -> Option<Shown> {
        let scalar = |v: &Value| match v {
            Value::Bool(b) => Some(Shown::Bool(*b)),
            Value::Int(n) => Some(Shown::Int(*n)),
            Value::Num(n) if n.is_finite() => Some(Shown::Num(*n)),
            Value::Str(s) if s.chars().count() <= MAX_SHOWN_TEXT => Some(Shown::Text(s.clone())),
            _ => None,
        };
        let fields = |map: &BTreeMap<String, Value>| -> Vec<(String, Shown)> { map.iter().take(32).filter_map(|(k, v)| scalar(v).map(|s| (k.clone(), s))).collect() };
        match value {
            Value::Map(map) => match map.get("msg") {
                Some(Value::Str(key)) if key.len() <= 128 => {
                    let args = match map.get("args") {
                        Some(Value::Map(args)) => fields(args),
                        _ => Vec::new(),
                    };
                    Some(Shown::Msg(key.clone(), args))
                }
                _ => None,
            },
            Value::List(rows) => Some(Shown::Rows(
                rows.iter()
                    .take(MAX_ROWS)
                    .filter_map(|row| match row {
                        Value::Map(map) => Some(fields(map)),
                        _ => None,
                    })
                    .collect(),
            )),
            other => scalar(other),
        }
    }

    /// As a value a condition compares (scalars only).
    fn val(&self) -> Option<Val> {
        match self {
            Shown::Bool(b) => Some(Val::Bool(*b)),
            Shown::Int(n) => Some(Val::Num(*n as f64)),
            Shown::Num(n) => Some(Val::Num(*n)),
            Shown::Text(s) => Some(Val::Text(s.clone())),
            Shown::Msg(..) | Shown::Rows(_) => None,
        }
    }

    fn put(&self, key: &str, state: &mut StateMap) {
        match self {
            Shown::Bool(b) => {
                state.set_bool(key, *b);
            }
            Shown::Int(n) => {
                state.set_int(key, *n);
            }
            Shown::Num(n) => {
                state.set_num(key, *n);
            }
            Shown::Text(s) => {
                state.set_str(key, s);
            }
            Shown::Msg(msg, args) => {
                let into = state.set_msg(key, msg);
                for (name, value) in args {
                    value.put(name, into);
                }
            }
            Shown::Rows(rows) => {
                let list = state.list_mut(key);
                list.clear();
                for row in rows {
                    let into = list.push();
                    for (name, value) in row {
                        value.put(name, into);
                    }
                }
            }
        }
    }

    fn to_json(&self) -> Json {
        match self {
            Shown::Bool(b) => Json::Bool(*b),
            Shown::Int(n) => Json::from(*n),
            Shown::Num(n) => Json::Num(*n),
            Shown::Text(s) => Json::from(s.as_str()),
            Shown::Msg(key, args) => Json::obj([("msg", Json::from(key.as_str())), ("args", Json::Obj(args.iter().map(|(k, v)| (k.clone(), v.to_json())).collect()))]),
            Shown::Rows(rows) => Json::arr(rows.iter().map(|row| Json::Obj(row.iter().map(|(k, v)| (k.clone(), v.to_json())).collect()))),
        }
    }
}

/// The interface: the toolkit's instance for the window, a second one for captures (so a
/// capture never touches what the player sees), and the state both are bound to.
struct Face {
    ui: Ui,
    overlay: GpuOverlay,
    capture: Option<(Ui, GpuOverlay)>,
    /// The names of the screens that loaded.
    screens: Vec<String>,
}

/// The project shell.
pub struct Shell {
    name: String,
    project: PathBuf,
    assets: PathBuf,
    game: GameFile,
    files: ShellFiles,
    /// What did not load or was not done, newest last (at most 16): `game_state` shows it.
    problems: Vec<String>,
    run: Run,
    settings: Settings,
    store: SettingsStore,
    settings_dirty: bool,
    /// The last input was the player's own (a window frame), not an agent's.
    human: bool,
    face: Option<Face>,
    state: StateMap,
    input: Input,
    /// An input that never changes: captures lay out with it (no hover, no presses).
    idle: Input,
    input_clock: Instant,
    display: Display,
    laid_out: Option<(u32, u32)>,
    /// The string tables, and each language's strings (made when the tables are read).
    tables: Vec<Arc<Table>>,
    strings: Vec<(String, Arc<Strings>)>,
    /// What the modules keep, as last seen: `<module>.<key>`.
    shown: Vec<(String, Shown)>,
    events_out: Vec<(String, Value)>,
    events_dropped: u64,
    clock_wish: Option<bool>,
    requests: Vec<GameRequest>,
    quit: bool,
    refused: u32,
    refused_told: u32,
    title_sent: bool,
    audio: Option<Arc<crate::audio_service::AudioService>>,
    ui_events: UiEvents,
}

fn note(problems: &mut Vec<String>, what: String) {
    // (Said once: a second interface loaded for captures finds the same broken file.)
    if problems.contains(&what) {
        return;
    }
    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "shell.problem", what = what.as_str());
    if problems.len() == 16 {
        problems.remove(0);
    }
    problems.push(what);
}

impl Shell {
    /// Whether `project` has a game file: the engine hosts the shell for it.
    pub fn wanted(project: &Path) -> bool {
        project.join(specula_ludus::game::FILE).is_file()
    }

    /// The shell of the project at `project`, read from its folder. `Err`: its game file is
    /// not one (the message says where and what to write); the engine does not start on a
    /// game it cannot read.
    pub fn open(name: &str, project: &Path, assets: &Path) -> Result<Shell, String> {
        let mut shell = Shell::build(name, ShellFiles::read(project, assets))?;
        shell.project = project.to_path_buf();
        shell.assets = assets.to_path_buf();
        Ok(shell)
    }

    /// The shell made from `files`.
    pub fn build(name: &str, files: ShellFiles) -> Result<Shell, String> {
        let game = GameFile::parse(&files.game).map_err(|p| format!("{}: {p}", specula_ludus::game::FILE))?;
        let mut map = ActionMap::new();
        specula_ui::add_ui_actions(&mut map).map_err(|e| format!("the interface's own actions: {e}"))?;
        let mut shell = Shell {
            name: name.to_owned(),
            project: PathBuf::new(),
            assets: PathBuf::new(),
            run: Run::new(&game),
            settings: Settings::new(&game.settings, std::slice::from_ref(&game.locale)),
            game,
            problems: Vec::new(),
            store: SettingsStore::default(),
            settings_dirty: false,
            human: false,
            face: None,
            state: StateMap::new(),
            input: Input::new(map.clone()),
            idle: Input::new(map),
            input_clock: Instant::now(),
            display: Display::new([1280, 720]),
            laid_out: None,
            tables: Vec::new(),
            strings: Vec::new(),
            shown: Vec::new(),
            events_out: Vec::new(),
            events_dropped: 0,
            clock_wish: None,
            requests: Vec::new(),
            quit: false,
            refused: 0,
            refused_told: 0,
            title_sent: false,
            audio: None,
            ui_events: UiEvents::default(),
            files: ShellFiles::default(),
        };
        shell.take_files(files);
        Ok(shell)
    }

    /// Takes the files as read: tables, then the interface. What does not load is noted and
    /// the rest goes on (a screen with a typo does not take the game's menus away).
    fn take_files(&mut self, files: ShellFiles) {
        for problem in &files.problems {
            note(&mut self.problems, problem.clone());
        }
        self.files = files;
        self.load_tables();
        self.load_face();
        self.apply_language();
        self.laid_out = None;
    }

    fn load_tables(&mut self) {
        let mut tables: Vec<Arc<Table>> = Vec::new();
        for (stem, text) in &self.files.tables {
            match Table::parse(text) {
                Ok(table) if tables.iter().any(|t| t.locale == table.locale) => note(&mut self.problems, format!("assets/strings/{stem}.json: a second table for {}", table.locale)),
                Ok(table) => tables.push(Arc::new(table)),
                Err(e) => note(&mut self.problems, format!("assets/strings/{stem}.json: {e}")),
            }
        }
        let refs: Vec<&Table> = tables.iter().map(|t| &**t).collect();
        let source = tables.iter().find(|t| t.source).cloned();
        let mut strings = Vec::new();
        for table in &tables {
            match Catalog::build(&table.locale, &refs) {
                Ok(catalog) => strings.push((table.locale.as_str().to_owned(), Arc::new(Strings::new(catalog, source.clone(), Vec::new())))),
                Err(e) => note(&mut self.problems, format!("the strings of {}: {e}", table.locale)),
            }
        }
        // The game's own language first, then the others by name: the order a `cycle` goes in.
        let own = self.game.locale.clone();
        let mut locales: Vec<String> = strings.iter().map(|(l, _)| l.clone()).collect();
        locales.sort_by_key(|l| (*l != own, l.clone()));
        if locales.is_empty() {
            locales.push(own);
        }
        let chosen = self.settings.locale().to_owned();
        let kept = self.settings.to_text();
        self.settings = Settings::new(&self.game.settings, &locales);
        self.settings.read_text(&kept);
        if self.settings.locale() != chosen && locales.contains(&chosen) {
            let _ = self.settings.set_text(&self.language_setting(), &chosen);
        }
        self.tables = tables;
        self.strings = strings;
    }

    /// The name of the game's language setting.
    fn language_setting(&self) -> String {
        self.settings.defs().iter().find(|d| d.kind == specula_ludus::SettingKind::Locale).map_or_else(|| "language".to_owned(), |d| d.name.clone())
    }

    fn load_ui(&mut self) -> Result<(Ui, Vec<String>), String> {
        let face = |b: specula_text::Bundled| b.load().map_err(|e| format!("bundled font {}: {e}", b.name()));
        let fonts = FontSet::new().with("ui", face(specula_text::Bundled::UiRegular)?).with("ui_bold", face(specula_text::Bundled::UiBold)?).with("title", face(specula_text::Bundled::Display)?);
        let theme = match Theme::from_json_str(&self.files.theme) {
            Ok(theme) => theme,
            Err(e) => {
                note(&mut self.problems, format!("assets/ui/theme.json: {e} (the built-in theme is used)"));
                Theme::from_json_str(builtin("assets/ui/theme.json")).map_err(|e| format!("the built-in theme: {e}"))?
            }
        };
        let mut ui = Ui::new(theme, fonts);
        let mut loaded = Vec::new();
        for (stem, text) in &self.files.screens {
            // A screen is known by its file's name: its own `name` must say the same.
            let named = Json::parse(text).ok().and_then(|j| j.get("name").and_then(Json::as_str).map(str::to_owned));
            if named.as_deref() != Some(stem.as_str()) {
                note(&mut self.problems, format!("assets/ui/{stem}.json: its \"name\" must be \"{stem}\", the file's own (it says {named:?}): not loaded"));
                continue;
            }
            match ui.load_screen(text) {
                Ok(_) => loaded.push(stem.clone()),
                Err(e) => note(&mut self.problems, format!("assets/ui/{stem}.json: {e}: not loaded")),
            }
        }
        Ok((ui, loaded))
    }

    fn load_face(&mut self) {
        match self.load_ui() {
            Ok((ui, screens)) => self.face = Some(Face { ui, overlay: GpuOverlay::new(), capture: None, screens }),
            Err(e) => {
                note(&mut self.problems, format!("no interface: {e}"));
                self.face = None;
            }
        }
    }

    /// Shows the chosen language in the window's interface and a capture's.
    fn apply_language(&mut self) {
        let locale = self.settings.locale().to_owned();
        let strings = self.strings.iter().find(|(l, _)| *l == locale).or_else(|| self.strings.first()).map(|(_, s)| Arc::clone(s));
        if let Some(face) = &mut self.face {
            face.ui.set_strings(strings.clone());
            if let Some((ui, _)) = &mut face.capture {
                ui.set_strings(strings);
            }
        }
        self.laid_out = None;
    }

    /// What a setting stands for in the engine, done: the language, the volume.
    fn apply_setting(&mut self, name: &str) {
        let Some(def) = self.settings.def(name).cloned() else { return };
        if def.kind == specula_ludus::SettingKind::Locale {
            self.apply_language();
        }
        if def.is.as_deref() == Some("volume") {
            self.apply_volume();
        }
    }

    fn apply_volume(&mut self) {
        let (Some(audio), Some(volume)) = (&self.audio, self.settings.fraction("volume")) else { return };
        if audio.enabled() {
            if let Err(e) = audio.set_bus_gain(&self.name, specula_audio::Bus::Master, volume as f32, 0.05) {
                note(&mut self.problems, format!("the volume: {e}"));
            }
        }
    }

    fn has_screen(&self, name: &str) -> bool {
        self.face.as_ref().is_some_and(|f| f.screens.iter().any(|s| s == name))
    }

    /// Does what the run answered.
    fn apply(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Clock(paused) => self.clock_wish = Some(paused),
                Effect::Event { name, data } => {
                    if self.events_out.len() >= MAX_EVENTS {
                        self.events_dropped += 1;
                        continue;
                    }
                    let to_value = |v: Val| match v {
                        Val::Bool(b) => Value::Bool(b),
                        Val::Num(n) if specula_core::dmath::fract_f64(n) == 0.0 && n.abs() < 9.0e15 => Value::Int(n as i64),
                        Val::Num(n) => Value::Num(n),
                        Val::Text(s) => Value::Str(s),
                    };
                    let data = Value::Map(data.into_iter().map(|(k, v)| (k.to_owned(), to_value(v))).collect());
                    self.events_out.push((name.to_owned(), data));
                }
                Effect::Setting { name } => {
                    // The player's own file keeps the player's own changes; an agent's are kept
                    // only where the operator named the settings folder (scripts, tests).
                    if self.human || self.store.agents_save {
                        self.settings_dirty = true;
                    }
                    self.apply_setting(&name);
                }
                Effect::Save(slot) => self.requests.push(GameRequest::Save { slot, label: None }),
                Effect::Load(slot) => self.requests.push(GameRequest::Load { slot }),
                Effect::Quit => self.quit = true,
            }
        }
        self.laid_out = None;
    }

    /// A button's word, done. `Err`: what was refused, and why.
    pub fn press(&mut self, word: &str, player: bool) -> Result<(), String> {
        let action = Action::parse(word)?;
        let screens: Vec<String> = self.face.as_ref().map(|f| f.screens.clone()).unwrap_or_default();
        let has = |name: &str| screens.iter().any(|s| s == name);
        match self.run.act(&action, &mut self.settings, player, &has) {
            Ok(effects) => {
                self.apply(effects);
                Ok(())
            }
            Err(refused) => {
                if refused.code == "players_only" {
                    self.refused += 1;
                }
                Err(format!("{} ({})", refused.message, refused.code))
            }
        }
    }

    /// What the modules keep, taken as screens and conditions read it.
    fn see(&mut self, modules: &ModuleStates) {
        self.shown.clear();
        'all: for (module, store) in modules.each() {
            for (key, value) in store {
                if self.shown.len() >= MAX_SHOWN {
                    break 'all;
                }
                if let Some(shown) = Shown::of(value) {
                    self.shown.push((format!("{module}.{key}"), shown));
                }
            }
        }
    }

    /// The value a key names, for a condition: a module's (`gate.hp`), a setting's
    /// (`setting.difficulty`), the run's own (`game.runs`).
    fn value(&self, key: &str) -> Option<Val> {
        if let Some(name) = key.strip_prefix("setting.") {
            return self.settings.get(name).cloned();
        }
        match key {
            "game.runs" => return Some(Val::Num(self.run.runs() as f64)),
            "game.phase" => return Some(Val::Text(self.run.phase().name().to_owned())),
            _ => {}
        }
        self.shown.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.val())
    }

    /// The score, when the game file names the key that holds it and it holds a number.
    fn score(&self) -> Option<f64> {
        match self.value(self.game.outcome.score.as_deref()?)? {
            Val::Num(n) => Some(n),
            _ => None,
        }
    }

    /// Fills the state the screens bind to: the run's own, the settings, what the modules keep.
    fn fill(&mut self) {
        let score = self.score();
        let language_name_key = format!("setting.{}.name", self.language_setting());
        let s = &mut self.state;
        s.reset();
        let title = if self.game.title.is_empty() { self.name.as_str() } else { self.game.title.as_str() };
        s.set_str("game.title", title);
        s.set_str("game.phase", self.run.phase().name());
        s.set_bool("game.paused", self.run.phase() == Phase::Paused);
        s.set_bool("game.running", self.run.phase() == Phase::Running);
        s.set_int("game.runs", self.run.runs() as i64);
        s.set_bool("game.has_score", score.is_some());
        if let Some(score) = score {
            s.set_num("game.score", score);
        }
        for def in self.settings.defs() {
            let key = format!("setting.{}", def.name);
            match self.settings.get(&def.name) {
                Some(Val::Bool(b)) => {
                    s.set_bool(&key, *b);
                }
                Some(Val::Num(n)) => {
                    s.set_int(&key, *n as i64);
                }
                Some(Val::Text(t)) => {
                    s.set_str(&key, t);
                }
                None => {}
            }
        }
        // The language's own name ("Deutsch"), as its table gives it.
        let locale = self.settings.locale();
        let name = self.tables.iter().find(|t| t.locale.as_str() == locale).map_or(locale, |t| t.name.as_str());
        s.set_str(&language_name_key, name);
        for (key, shown) in &self.shown {
            shown.put(key, s);
        }
    }

    /// Shows the screens the run says are up, and hides the rest.
    fn sync_screens(&mut self) {
        let Some(face) = &mut self.face else { return };
        let up: Vec<String> = self.run.visible().into_iter().map(str::to_owned).collect();
        for name in &face.screens {
            let want = up.iter().any(|u| u == name);
            if face.ui.is_visible(name) != want {
                let _ = if want { face.ui.show(name) } else { face.ui.hide(name) };
            }
        }
    }

    fn viewport(ui: &Ui, display: &Display, size: (u32, u32)) -> specula_ui::Viewport {
        Display { size: [size.0.max(1), size.1.max(1)], ..*display }.viewport(ui.theme(), 1.0)
    }

    /// The presses of one laid-out frame, done.
    fn handle(&mut self, events: &UiEvents, player: bool) {
        let mut backed = false;
        for event in &events.events {
            match event {
                UiEvent::Action { action, .. } => {
                    backed |= matches!(&**action, "back" | "resume" | "close");
                    if let Err(why) = self.press(action, player) {
                        self.say(&why, ToastKind::Warn);
                    }
                }
                // A menu's "back" with no action of its own.
                UiEvent::Back { .. } if !backed => {
                    backed = true;
                    let _ = self.press("back", player);
                }
                UiEvent::Back { .. } | UiEvent::Refused { .. } => {}
            }
        }
        // Back while only the game's own screen is up (no menu took it): the pause menu.
        if !backed && !events.consumed.navigation && self.input.pressed(PAUSE) && self.run.phase() == Phase::Running && self.run.stack().is_empty() {
            let _ = self.press("pause", player);
        }
    }

    /// A capture's interface, from a second instance of the toolkit (loaded at the first
    /// capture): the window's screens and the player's focus, laid out for the capture's size
    /// with no input, so a capture never touches what the player sees.
    fn capture_overlay(&mut self, out: &mut Overlay) {
        if self.face.as_ref().is_some_and(|f| f.capture.is_none()) {
            let loaded = self.load_ui();
            let strings = self.face.as_ref().and_then(|f| f.ui.strings().cloned());
            match (loaded, &mut self.face) {
                (Ok((mut ui, _)), Some(face)) => {
                    ui.set_strings(strings);
                    face.capture = Some((ui, GpuOverlay::new()));
                }
                (Err(e), _) => {
                    note(&mut self.problems, format!("a capture's interface: {e}"));
                    return;
                }
                (Ok(_), None) => return,
            }
        }
        let Some(Face { ui: window, capture: Some((ui, overlay)), screens, .. }) = &mut self.face else { return };
        for name in screens.iter() {
            let open = window.is_visible(name);
            if ui.is_visible(name) != open {
                let _ = if open { ui.show(name) } else { ui.hide(name) };
            }
        }
        let view = Shell::viewport(ui, &self.display, out.size);
        ui.copy_interaction_from(window, view.scale);
        ui.frame(&self.idle, &self.state, &view);
        if overlay.sync_atlas(out.textures, ui.atlas_mut()).is_err() {
            return;
        }
        if let Ok(quads) = overlay.build(ui) {
            out.quads.extend_from_slice(quads);
        }
    }

    fn say(&mut self, text: &str, kind: ToastKind) {
        if let Some(face) = &mut self.face {
            face.ui.toast(text, kind);
        }
    }

    /// The run as data: `game_state`.
    fn state_json(&self) -> Json {
        let mut j = Json::obj([
            ("game", Json::from(if self.game.title.is_empty() { self.name.as_str() } else { self.game.title.as_str() })),
            ("phase", Json::from(self.run.phase().name())),
            ("outcome", Json::from(match self.run.phase() {
                Phase::Ended(how) => how.name(),
                _ => Ended::Running.name(),
            })),
            ("runs", Json::from(self.run.runs())),
            ("tick_hz", Json::from(self.game.tick_hz)),
            ("screens", Json::arr(self.run.visible().into_iter().filter(|s| self.has_screen(s)).map(Json::from))),
            ("locale", Json::from(self.settings.locale())),
        ]);
        if let Some(tick) = self.run.ended_at() {
            j.set("ended_at", Json::from(tick));
        }
        if let Some(score) = self.score() {
            j.set("score", Json::Num(score));
        }
        if let Some(key) = &self.game.outcome.standings {
            if let Some((_, rows)) = self.shown.iter().find(|(k, _)| k == key) {
                j.set("standings", rows.to_json());
            }
        }
        j.set("settings", Json::Obj(self.settings.defs().iter().filter_map(|d| self.settings.get(&d.name).map(|v| (d.name.clone(), v.to_json()))).collect()));
        // What the game's conditions read, as they stand: why the run is where it is.
        let mut reads: Vec<&str> = self.game.outcome.keys();
        reads.sort_unstable();
        reads.dedup();
        j.set("reads", Json::Obj(reads.into_iter().map(|k| (k.to_owned(), self.value(k).map_or(Json::Null, |v| v.to_json()))).collect()));
        j.set("modules", Json::arr(self.game.modules.iter().map(|m| Json::from(m.as_str()))));
        if !self.events_out.is_empty() || self.events_dropped > 0 {
            j.set("events_waiting", Json::from(self.events_out.len()));
            if self.events_dropped > 0 {
                j.set("events_dropped", Json::from(self.events_dropped));
            }
        }
        if !self.problems.is_empty() {
            j.set("problems", Json::arr(self.problems.iter().map(|p| Json::from(p.as_str()))));
        }
        j
    }
}

/// The run's part of a save game: where it stood.
const SAVE_FORMAT: &str = "specula.shell.save.v1";

impl Game for Shell {
    fn name(&self) -> &str {
        &self.name
    }

    fn tick_hz(&self) -> u32 {
        self.game.tick_hz
    }

    fn use_audio(&mut self, audio: Arc<crate::audio_service::AudioService>) {
        self.audio = Some(audio);
        self.apply_volume();
    }

    fn start(&mut self, _ctx: &mut Ctx) {
        let effects = self.run.began(&self.settings);
        self.apply(effects);
    }

    fn after_modules(&mut self, ctx: &mut Ctx, modules: &ModuleStates) {
        self.see(modules);
        let effects = {
            let look = |key: &str| self.value(key);
            let mut run = self.run.clone();
            let effects = run.after_tick(&look, ctx.clock.tick());
            self.run = run;
            effects
        };
        if !effects.is_empty() {
            self.apply(effects);
        }
    }

    fn see_modules(&mut self, modules: &ModuleStates) {
        self.see(modules);
    }

    fn take_module_events(&mut self, out: &mut Vec<(String, Value)>) {
        out.append(&mut self.events_out);
    }

    fn take_clock(&mut self) -> Option<bool> {
        self.clock_wish.take()
    }

    fn clock_seen(&mut self, paused: bool) {
        self.run.clock_seen(paused);
        self.laid_out = None;
    }

    fn game_state(&self) -> Option<Json> {
        Some(self.state_json())
    }

    fn frame(&mut self, _ctx: &mut Ctx, frame: &mut Frame) {
        let mut pads = [GamepadState::default(); MAX_GAMEPADS];
        for (slot, pad) in pads.iter_mut().zip(frame.pads) {
            *slot = *pad;
        }
        // This clock only measures frame time for input (taps, holds), never the simulation.
        let now = self.input_clock.elapsed().as_secs_f64();
        if frame.synthetic {
            self.input.update_synthetic(now, frame.input, &pads);
        } else {
            self.input.update(now, frame.input, &pads);
        }
        self.human = !frame.synthetic;
        if !frame.synthetic && !self.title_sent {
            *frame.title = Some(if self.game.title.is_empty() { self.name.clone() } else { self.game.title.clone() });
            self.title_sent = true;
        }
        self.idle.set_last_device(self.input.last_device());
        self.idle.set_pad_style(self.input.pad_style());
        if frame.size.0 == 0 || frame.size.1 == 0 {
            return;
        }
        self.fill();
        self.sync_screens();
        let mut events = std::mem::take(&mut self.ui_events);
        events.events.clear();
        events.consumed = Default::default();
        if let Some(face) = &mut self.face {
            let view = Shell::viewport(&face.ui, &self.display, frame.size);
            events.clone_from(face.ui.frame(&self.input, &self.state, &view).events);
            self.laid_out = Some(frame.size);
        }
        self.handle(&events, !frame.synthetic);
        self.ui_events = events;
    }

    fn overlay(&mut self, _ctx: &mut Ctx, out: &mut Overlay) {
        self.fill();
        self.sync_screens();
        if out.capture {
            self.capture_overlay(out);
            return;
        }
        let Some(face) = &mut self.face else { return };
        if self.laid_out != Some(out.size) {
            let view = Shell::viewport(&face.ui, &self.display, out.size);
            face.ui.frame(&self.idle, &self.state, &view);
            // The window lays out again for itself on its next frame.
            self.laid_out = None;
        }
        if face.overlay.sync_atlas(out.textures, face.ui.atlas_mut()).is_err() {
            return;
        }
        if let Ok(quads) = face.overlay.build(&face.ui) {
            out.quads.extend_from_slice(quads);
        }
    }

    fn display(&mut self, display: &Display) {
        if self.display != *display {
            self.display = *display;
            self.laid_out = None;
        }
    }

    fn locale(&self) -> GameLocale {
        GameLocale { locale: Some(self.settings.locale().to_owned()), keys: None }
    }

    fn ui(&mut self, pick: UiPick) -> Option<GameUi<'_>> {
        if pick == UiPick::Live {
            self.fill();
            self.sync_screens();
        }
        let face = self.face.as_mut()?;
        let ui = match (pick, &mut face.capture) {
            (UiPick::Capture, Some((ui, _))) => ui,
            _ => &mut face.ui,
        };
        Some(GameUi { ui, state: &self.state, user_scale: 1.0, input_map: Some(self.input.map()) })
    }

    fn use_settings(&mut self, text: Option<&str>, store: &SettingsStore) {
        self.store = store.clone();
        if let Some(text) = text {
            for problem in self.settings.read_text(text) {
                note(&mut self.problems, format!("the saved settings: {problem}"));
            }
        }
        self.apply_language();
        self.apply_volume();
    }

    fn take_settings(&mut self) -> Option<String> {
        let dirty = std::mem::take(&mut self.settings_dirty);
        (dirty && self.store.saving()).then(|| self.settings.to_text())
    }

    fn take_requests(&mut self, out: &mut Vec<GameRequest>) {
        out.append(&mut self.requests);
    }

    fn request_done(&mut self, _ctx: &mut Ctx, request: &GameRequest, result: Result<Json, Json>) {
        if let Err(e) = result {
            let what = match request {
                GameRequest::Save { slot, .. } => format!("saving to {slot}"),
                GameRequest::Load { slot } => format!("loading {slot}"),
                GameRequest::List | GameRequest::Delete { .. } => "the save games".to_owned(),
            };
            let why = e.get("message").and_then(Json::as_str).unwrap_or("it was refused").to_owned();
            note(&mut self.problems, format!("{what}: {why}"));
            self.say(&why, ToastKind::Warn);
        }
    }

    fn take_quit(&mut self) -> bool {
        std::mem::take(&mut self.quit)
    }

    fn take_refused_presses(&mut self) -> u32 {
        let n = self.refused - self.refused_told;
        self.refused_told = self.refused;
        n
    }

    fn modal_changed(&mut self, _ctx: &mut Ctx, open: bool) {
        // An engine screen took the window: a run under way pauses, so the person does not
        // lose while deciding.
        if open && self.run.phase() == Phase::Running {
            let _ = self.press("pause", true);
        }
    }

    fn assets_changed(&mut self, _ctx: &mut Ctx, paths: &[String]) {
        if self.assets.as_os_str().is_empty() || !paths.iter().any(|p| p.starts_with("ui/") || p.starts_with("strings/")) {
            return;
        }
        let mut files = ShellFiles::read(&self.project, &self.assets);
        // The game file is read at start: a change to it takes a restart (said in game_state's
        // problems if it no longer parses).
        files.game = std::mem::take(&mut self.files.game);
        self.take_files(files);
    }

    fn save_state(&self) -> Option<Result<Vec<u8>, String>> {
        let mut j = Json::obj([("format", Json::from(SAVE_FORMAT)), ("phase", Json::from(self.run.phase().name())), ("runs", Json::from(self.run.runs()))]);
        if let Some(tick) = self.run.ended_at() {
            j.set("ended_at", Json::from(tick));
        }
        Some(Ok(j.to_string().into_bytes()))
    }

    fn parse_state(&self, data: &[u8]) -> Result<Box<dyn std::any::Any>, String> {
        let text = std::str::from_utf8(data).map_err(|_| "the game's part of the save is not text".to_owned())?;
        let j = Json::parse(text).map_err(|e| format!("the game's part of the save is not JSON ({e})"))?;
        if j.get("format").and_then(Json::as_str) != Some(SAVE_FORMAT) {
            return Err(format!("the game's part of the save is not \"{SAVE_FORMAT}\""));
        }
        let phase = match j.get("phase").and_then(Json::as_str) {
            Some("title") => Phase::Title,
            Some("running") => Phase::Running,
            Some("paused") => Phase::Paused,
            Some(other) => Phase::Ended(Ended::named(other).filter(|e| *e != Ended::Running).ok_or_else(|| format!("the save names a phase this game does not have: {other}"))?),
            None => return Err("the game's part of the save names no phase".to_owned()),
        };
        Ok(Box::new((phase, j.get("ended_at").and_then(Json::as_u64))))
    }

    fn apply_state(&mut self, ctx: &mut Ctx, state: Box<dyn std::any::Any>) {
        if let Ok(saved) = state.downcast::<(Phase, Option<u64>)>() {
            self.run.restore(saved.0, saved.1);
            // The clock is as the phase says: only a run under way ticks.
            self.clock_wish = Some(saved.0 != Phase::Running);
            let _ = ctx;
            self.laid_out = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: &str = r#"{
        "format": "specula.game.v1",
        "title": "Hold the gate",
        "tick_hz": 60,
        "modules": ["modules/gate"],
        "settings": [{ "name": "difficulty", "kind": "choice", "options": ["easy", "hard"], "sim": true }],
        "outcome": { "lost": { "key": "gate.hp", "lte": 0 }, "won": "gate.won", "score": "gate.score" }
    }"#;

    fn shell() -> Shell {
        Shell::build("gate", ShellFiles::builtin(GAME)).expect("the built-in files make a shell")
    }

    fn events(shell: &mut Shell) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        shell.take_module_events(&mut out);
        out
    }

    /// The files a new project is given load, every one: the theme, the five screens under
    /// their own names, the words.
    #[test]
    fn the_built_in_files_make_a_whole_interface() {
        let shell = shell();
        assert_eq!(shell.problems, Vec::<String>::new());
        let face = shell.face.as_ref().expect("an interface");
        let mut loaded = face.screens.clone();
        loaded.sort();
        assert_eq!(loaded, ["hud", "lost", "pause", "settings", "title", "won"]);
        assert_eq!(shell.tick_hz(), 60);
        assert_eq!(shell.strings.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(), ["en"]);
        // Every screen's file names itself, and every word a screen shows is in the table.
        let table = shell.tables.iter().find(|t| t.locale.as_str() == "en").unwrap();
        for (path, text) in BUILTIN.iter().filter(|(p, _)| p.starts_with("assets/ui/") && !p.ends_with("theme.json")) {
            for (at, _) in text.match_indices("\"@") {
                let key: String = text[at + 2..].chars().take_while(|c| *c != '"').collect();
                assert!(table.entries.contains_key(&key), "{path} shows @{key}, which assets/strings/en.json does not have");
            }
        }
    }

    #[test]
    fn a_game_file_that_is_wrong_stops_the_start_with_where() {
        let wrong = Shell::build("x", ShellFiles::builtin(r#"{"format": "specula.game.v1", "tick_rate": 60}"#)).err().expect("refused");
        assert!(wrong.starts_with("game.json: tick_rate:") && wrong.contains("tick_hz"), "{wrong}");
        assert!(Shell::build("x", ShellFiles::builtin("")).is_err(), "no game file is no game");
    }

    #[test]
    fn the_title_holds_the_clock_and_start_tells_the_modules() {
        let mut shell = shell();
        let effects = shell.run.began(&shell.settings);
        shell.apply(effects);
        assert_eq!(shell.take_clock(), Some(true), "the title holds the clock");
        assert_eq!(shell.take_clock(), None, "asked once");
        shell.fill();
        shell.sync_screens();
        let face = shell.face.as_ref().unwrap();
        assert_eq!(face.ui.visible_screens().collect::<Vec<_>>(), ["title"]);
        // Start: the clock runs, and the modules hear the settings the rules read, then `run`.
        shell.press("start", true).unwrap();
        assert_eq!(shell.take_clock(), Some(false));
        let heard = events(&mut shell);
        assert_eq!(heard.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["setting", "run"]);
        assert_eq!(heard[0].1, Value::map().with("name", "difficulty").with("value", "easy"));
        assert_eq!(heard[1].1, Value::map().with("phase", "start").with("run", 1i64));
        shell.sync_screens();
        assert_eq!(shell.face.as_ref().unwrap().ui.visible_screens().collect::<Vec<_>>(), ["hud"]);
        assert_eq!(shell.game_state().unwrap().get("phase").and_then(Json::as_str), Some("running"));
    }

    #[test]
    fn a_games_own_button_reaches_its_modules_and_a_bad_word_is_refused() {
        let mut shell = shell();
        shell.press("start", true).unwrap();
        events(&mut shell);
        shell.press("build_wall", false).unwrap();
        assert_eq!(events(&mut shell), [("ui".to_owned(), Value::map().with("action", "build_wall"))]);
        assert!(shell.press("open:nowhere", true).unwrap_err().contains("no_screen"));
        assert!(shell.press("open:", true).unwrap_err().contains("open:settings"), "a shell word said wrongly says what it takes");
        // Quit is the player's alone, and an agent's try is counted for the input monitor.
        assert!(shell.press("quit", false).unwrap_err().contains("players_only"));
        assert_eq!((shell.take_refused_presses(), shell.take_refused_presses(), shell.take_quit()), (1, 0, false));
        shell.press("quit", true).unwrap();
        assert!(shell.take_quit());
        // Events do not pile up without bound while nothing ticks.
        for _ in 0..MAX_EVENTS + 10 {
            shell.press("fire", true).unwrap();
        }
        assert_eq!((shell.events_out.len(), shell.events_dropped), (MAX_EVENTS, 10));
    }

    #[test]
    fn settings_are_kept_for_the_player_and_not_an_agents_changes() {
        let mut shell = shell();
        shell.use_settings(None, &SettingsStore { kind: "per_user", agents_save: false, problem: None });
        assert_eq!(shell.take_settings(), None, "nothing changed yet");
        // An agent's change is the session's: shown, not saved to the player's file.
        shell.press("cycle:difficulty", false).unwrap();
        assert_eq!(shell.take_settings(), None);
        assert_eq!(shell.game_state().unwrap().get("settings").and_then(|s| s.get("difficulty")).and_then(Json::as_str), Some("hard"));
        // The player's own change is saved, once.
        shell.human = true;
        shell.press("step:volume:-1", true).unwrap();
        let text = shell.take_settings().expect("the settings file");
        assert!(text.contains("\"volume\": 7") && text.contains("specula.settings.v1"), "{text}");
        assert_eq!(shell.take_settings(), None);
        // A new shell given that file has the values back.
        let mut again = super::tests::shell();
        again.use_settings(Some(&text), &SettingsStore { kind: "per_user", agents_save: false, problem: None });
        assert_eq!(again.settings.get("volume"), Some(&Val::Num(7.0)));
        // With settings off nothing is ever handed over.
        let mut off = super::tests::shell();
        off.human = true;
        off.press("step:volume:-1", true).unwrap();
        assert_eq!(off.take_settings(), None);
    }

    #[test]
    fn what_a_module_keeps_is_what_a_screen_is_given() {
        assert_eq!(Shown::of(&Value::Int(3)), Some(Shown::Int(3)));
        assert_eq!(Shown::of(&Value::Str("x".repeat(MAX_SHOWN_TEXT + 1))), None, "a long text is not for a label");
        assert_eq!(Shown::of(&Value::Null), None);
        let msg = Value::map().with("msg", "hud.wave").with("args", Value::map().with("wave", 2i64).with("of", 5i64));
        assert_eq!(Shown::of(&msg), Some(Shown::Msg("hud.wave".into(), vec![("of".into(), Shown::Int(5)), ("wave".into(), Shown::Int(2))])));
        let rows = Value::List(vec![Value::map().with("name", "ann").with("time", 12.5), Value::Int(1)]);
        assert_eq!(Shown::of(&rows), Some(Shown::Rows(vec![vec![("name".into(), Shown::Text("ann".into())), ("time".into(), Shown::Num(12.5))]])));
        // In the state a screen binds to, and as a condition reads it.
        let mut shell = shell();
        shell.shown = vec![("gate.hp".into(), Shown::Int(3)), ("gate.score".into(), Shown::Num(40.0)), ("gate.wave".into(), Shown::of(&msg).unwrap()), ("gate.order".into(), Shown::of(&rows).unwrap())];
        shell.fill();
        assert_eq!(shell.state.get("gate.hp").and_then(|v| v.as_f64()), Some(3.0));
        assert_eq!(shell.state.get("game.score").and_then(|v| v.as_f64()), Some(40.0));
        assert!(shell.state.get("game.has_score").is_some_and(|v| v.truthy()));
        assert_eq!(shell.state.list("gate.order").map(|l| l.len()), Some(1));
        assert_eq!(shell.state.get("setting.volume").and_then(|v| v.as_f64()), Some(8.0));
        assert!(shell.state.get("setting.language.name").is_some(), "the language's own name");
        assert_eq!((shell.value("gate.hp"), shell.value("setting.difficulty"), shell.value("gate.wave")), (Some(Val::Num(3.0)), Some(Val::Text("easy".into())), None));
        let state = shell.game_state().unwrap();
        assert_eq!(state.get("score").and_then(Json::as_f64), Some(40.0));
        assert_eq!(state.get("reads").map(Json::to_string).as_deref(), Some(r#"{"gate.hp":3,"gate.score":40,"gate.won":null}"#));
    }

    #[test]
    fn the_run_ends_when_the_modules_state_says_so_and_a_save_keeps_where_it_stood() {
        let mut shell = shell();
        shell.press("start", true).unwrap();
        shell.take_clock();
        events(&mut shell);
        // (after_modules, without an engine: what it does once it has seen the modules.)
        shell.shown = vec![("gate.hp".into(), Shown::Int(0))];
        let mut run = shell.run.clone();
        let effects = run.after_tick(&|key: &str| shell.value(key), 77);
        shell.run = run;
        shell.apply(effects);
        assert_eq!(shell.take_clock(), Some(true), "the clock stops with the run");
        assert_eq!(events(&mut shell), [("outcome".to_owned(), Value::map().with("outcome", "lost").with("tick", 77i64))]);
        let state = shell.game_state().unwrap();
        assert_eq!((state.get("outcome").and_then(Json::as_str), state.get("ended_at").and_then(Json::as_u64)), (Some("lost"), Some(77)));
        shell.sync_screens();
        assert_eq!(shell.face.as_ref().unwrap().ui.visible_screens().collect::<Vec<_>>(), ["hud", "lost"]);
        // The game's part of a save, and back.
        let bytes = shell.save_state().unwrap().unwrap();
        let parsed = shell.parse_state(&bytes).unwrap();
        let saved = parsed.downcast::<(Phase, Option<u64>)>().unwrap();
        assert_eq!(*saved, (Phase::Ended(Ended::Lost), Some(77)));
        assert!(shell.parse_state(b"{}").is_err() && shell.parse_state(br#"{"format": "specula.shell.save.v1", "phase": "flying"}"#).is_err());
    }

    /// A project's own files win, a part it lacks is the built-in one, and a file that does not
    /// load is said without taking the rest away.
    #[test]
    fn a_projects_files_are_read_with_the_built_in_ones_for_what_it_lacks() {
        let dir = std::env::temp_dir().join(format!("specula-shell-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let assets = dir.join("assets");
        std::fs::create_dir_all(assets.join("ui")).unwrap();
        std::fs::create_dir_all(assets.join("strings")).unwrap();
        std::fs::write(dir.join("game.json"), GAME).unwrap();
        std::fs::write(assets.join("ui").join("hud.json"), builtin("assets/ui/hud.json").replace("@hud.pause", "@hud.stop")).unwrap();
        std::fs::write(assets.join("ui").join("shop.json"), builtin("assets/ui/pause.json").replace("\"name\": \"pause\"", "\"name\": \"shop\"")).unwrap();
        std::fs::write(assets.join("ui").join("broken.json"), "{ not json").unwrap();
        std::fs::write(assets.join("ui").join("misnamed.json"), builtin("assets/ui/pause.json")).unwrap();
        std::fs::write(assets.join("strings").join("es.json"), builtin("assets/strings/en.json").replace("\"locale\": \"en\"", "\"locale\": \"es\"").replace("\"English\"", "\"Espa\u{f1}ol\"").replace("\"source\": true,", "")).unwrap();
        std::fs::write(assets.join("strings").join("en.json"), builtin("assets/strings/en.json")).unwrap();
        assert!(Shell::wanted(&dir) && !Shell::wanted(&assets));
        let mut shell = Shell::open("gate", &dir, &assets).unwrap();
        let mut loaded = shell.face.as_ref().unwrap().screens.clone();
        loaded.sort();
        assert_eq!(loaded, ["hud", "lost", "pause", "settings", "shop", "title", "won"]);
        assert!(shell.files.screens.iter().any(|(stem, text)| stem == "hud" && text.contains("@hud.stop")), "the project's own hud");
        assert_eq!(shell.problems.len(), 2, "{:?}", shell.problems);
        assert!(shell.problems.iter().any(|p| p.contains("broken.json")) && shell.problems.iter().any(|p| p.contains("misnamed.json") && p.contains("\"misnamed\"")));
        // Two languages, the game's own first; the choice is shown by its own name.
        assert_eq!(shell.settings.locales(), ["en", "es"]);
        shell.press("cycle:language", true).unwrap();
        shell.fill();
        assert_eq!(shell.settings.locale(), "es");
        assert_eq!(shell.state.get("setting.language").map(|v| format!("{v:?}")).is_some(), true);
        // The project's own screen opens by its name.
        shell.press("open:shop", true).unwrap();
        shell.sync_screens();
        assert!(shell.face.as_ref().unwrap().ui.is_visible("shop"));
        let _ = std::fs::remove_dir_all(dir);
    }
}

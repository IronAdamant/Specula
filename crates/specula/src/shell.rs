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
use specula_ludus::{Action, Effect, GameFile, Kept, Phase, Run, Settings, Val};
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
/// The text's bound is a label's own (`specula_ui::state::MAX_TEXT`, in bytes): a text is
/// copied here only when it changes and compared in place on every tick, and what the modules
/// keep is already at most `logic::handler::MAX_STATE_BYTES` a module, so the bound is what a
/// label can show whole, not what this copy costs. (It was 256 characters, from when the shell
/// was first built, with no reason given: a summary of a day of sixteen lines in one kept text
/// was dropped, and the screen showed `{work.disp}`; 2026-11-23, five of ten builders' games.)
const MAX_SHOWN: usize = 512;
const MAX_SHOWN_TEXT: usize = specula_ui::state::MAX_TEXT;
const MAX_ROWS: usize = 64;
/// Most of the values the modules keep and the screens are not given that the shell lists
/// with why ([`Unshown`]; `game_state`'s `not_shown`).
const MAX_UNSHOWN: usize = 32;
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
        files.problems = problems;
        files.with_defaults()
    }

    /// The shell's files from the texts a project's store holds under its assets (`ui/hud.json`,
    /// `strings/en.json`), with `game` as the game file: what a changed screen, theme or table
    /// is taken from while the engine runs. The same files as [`ShellFiles::read`] takes from
    /// the disk, by the same rule: each folder's files by name, the first [`MAX_SCREENS`] and
    /// [`MAX_TABLES`] of them, and it is said when there are more (review R2 of M6 part 3: the
    /// first so many texts of both folders together were taken, in the store's order, and
    /// past that the screens fell away with nothing said).
    pub fn of_texts(game: String, texts: &[(String, Arc<str>)]) -> ShellFiles {
        let mut files = ShellFiles { game, ..ShellFiles::default() };
        // (A file's name without its ending, and its text.)
        type Found<'t> = Vec<(&'t str, &'t Arc<str>)>;
        let (mut screens, mut tables): (Found, Found) = (Vec::new(), Vec::new());
        for (rel, text) in texts {
            match rel.strip_suffix(".json").and_then(|path| path.split_once('/')) {
                Some(("ui", "theme")) if text.len() as u64 <= MAX_FILE => files.theme = text.to_string(),
                Some(("ui", "theme")) => files.problems.push(format!("assets/{rel} is {} bytes: at most {MAX_FILE} are read", text.len())),
                Some(("ui", stem)) if !stem.contains('/') => screens.push((stem, text)),
                Some(("strings", stem)) if !stem.contains('/') => tables.push((stem, text)),
                _ => {}
            }
        }
        let mut folder = |name: &str, most: usize, mut found: Found| -> Vec<(String, String)> {
            found.sort_by_key(|(stem, _)| *stem);
            if found.len() > most {
                files.problems.push(format!("assets/{name} holds {} files: the first {most} are read", found.len()));
                found.truncate(most);
            }
            let mut out = Vec::new();
            for (stem, text) in found {
                if text.len() as u64 > MAX_FILE {
                    files.problems.push(format!("assets/{name}/{stem}.json is {} bytes: at most {MAX_FILE} are read", text.len()));
                } else {
                    out.push((stem.to_owned(), text.to_string()));
                }
            }
            out
        };
        let (screens, tables) = (folder("ui", MAX_SCREENS, screens), folder("strings", MAX_TABLES, tables));
        files.screens = screens;
        files.tables = tables;
        files.with_defaults()
    }

    /// These files with the built-in ones for what a project lacks: the theme, the default
    /// screens (by name), and the English words when it has no table at all.
    pub fn with_defaults(mut self) -> ShellFiles {
        let own = ShellFiles::builtin("");
        if self.theme.is_empty() {
            self.theme = own.theme;
        }
        for (stem, text) in own.screens {
            if !self.screens.iter().any(|(have, _)| *have == stem) {
                self.screens.push((stem, text));
            }
        }
        if self.tables.is_empty() {
            self.tables = own.tables;
        }
        self
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
            Value::Str(s) if s.len() <= MAX_SHOWN_TEXT => Some(Shown::Text(s.clone())),
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

    /// Whether `value`, shown, would be this: what [`Shown::of`] makes of it, without making
    /// it. What the modules keep is looked at on every tick and before every frame, and nearly
    /// all of it is as it was: an unchanged text, message or table costs a comparison here and
    /// no copy (review R1 of M6 part 3, and R2: every text and map was still copied each time).
    fn is(&self, value: &Value) -> bool {
        fn shows(v: &Value) -> bool {
            match v {
                Value::Bool(_) | Value::Int(_) => true,
                Value::Num(n) => n.is_finite(),
                Value::Str(s) => s.len() <= MAX_SHOWN_TEXT,
                _ => false,
            }
        }
        fn scalar(shown: &Shown, v: &Value) -> bool {
            match (shown, v) {
                (Shown::Bool(a), Value::Bool(b)) => a == b,
                (Shown::Int(a), Value::Int(b)) => a == b,
                (Shown::Num(a), Value::Num(b)) => b.is_finite() && a == b,
                // (Equal to a text that is shown: so no longer than one.)
                (Shown::Text(a), Value::Str(b)) => a == b,
                _ => false,
            }
        }
        fn fields(have: &[(String, Shown)], map: &BTreeMap<String, Value>) -> bool {
            let mut want = map.iter().take(32).filter(|(_, v)| shows(v));
            have.iter().all(|(key, shown)| want.next().is_some_and(|(k, v)| k == key && scalar(shown, v))) && want.next().is_none()
        }
        match (self, value) {
            (Shown::Msg(key, args), Value::Map(map)) => {
                matches!(map.get("msg"), Some(Value::Str(k)) if k.len() <= 128 && k == key)
                    && match map.get("args") {
                        Some(Value::Map(found)) => fields(args, found),
                        _ => args.is_empty(),
                    }
            }
            (Shown::Rows(rows), Value::List(list)) => {
                let mut want = list.iter().take(MAX_ROWS).filter_map(|row| match row {
                    Value::Map(map) => Some(map),
                    _ => None,
                });
                rows.iter().all(|row| want.next().is_some_and(|map| fields(row, map))) && want.next().is_none()
            }
            (shown, v) => scalar(shown, v),
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

/// Why a value a module keeps is not given to the screens: a screen that binds it shows its
/// `{module.key}` as written, and a run says this, not that nothing keeps it (2026-11-23: a
/// summary over the length was dropped with no word, and the run's warning sent the builder
/// to look for a `cx.set_state` that was there).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Unshown {
    /// A text longer than [`MAX_SHOWN_TEXT`] bytes: its length.
    Long(usize),
    /// A kind a screen cannot show, in words.
    Kind(&'static str),
    /// Past the first [`MAX_SHOWN`] values the modules keep.
    Past,
}

impl Unshown {
    /// Why `value` is not shown ([`Shown::of`] found nothing in it).
    fn of(value: &Value) -> Unshown {
        match value {
            Value::Str(s) => Unshown::Long(s.len()),
            Value::Null => Unshown::Kind("null"),
            Value::Num(_) => Unshown::Kind("a number that is not finite"),
            Value::Map(map) => match map.get("msg") {
                Some(Value::Str(_)) => Unshown::Kind("a map whose \"msg\" is longer than a string-table key (128 bytes)"),
                _ => Unshown::Kind("a map without a \"msg\" (a line of the string tables)"),
            },
            // (Shown always: a list is rows, a scalar is itself.)
            Value::Bool(_) | Value::Int(_) | Value::List(_) => Unshown::Kind("a value"),
        }
    }

    /// What is wrong and what to do, in words.
    fn words(&self) -> String {
        match self {
            Unshown::Long(bytes) => format!(
                "a text of {bytes} bytes, longer than the {MAX_SHOWN_TEXT} a screen is given: keep it shorter, or keep it in parts (several keys, or a list of maps a screen shows as rows)"
            ),
            Unshown::Kind(kind) => format!("{kind}, which a screen cannot show: keep a text, a number, true or false, a map with \"msg\" and \"args\", or a list of maps (rows)"),
            Unshown::Past => format!("past the first {MAX_SHOWN} values the modules keep, which are all the screens are given: keep fewer keys (what belongs together under one, as a list or a map)"),
        }
    }
}

/// Whether `have` is `module.key`, made of the two without a copy.
fn is_key(have: &str, module: &str, key: &str) -> bool {
    have.len() == module.len() + 1 + key.len() && have.starts_with(module) && have.as_bytes()[module.len()] == b'.' && have.ends_with(key)
}

/// The interface: the toolkit's instance for the window, a second one for captures (so a
/// capture never touches what the player sees), and the state both are bound to.
struct Face {
    ui: Ui,
    overlay: GpuOverlay,
    capture: Option<(Ui, GpuOverlay)>,
    /// The names of the screens that loaded.
    screens: Vec<String>,
    /// Each one's layer, as its file says it (0 when it says none).
    layers: BTreeMap<String, f64>,
}

/// The project shell.
pub struct Shell {
    name: String,
    game: GameFile,
    files: ShellFiles,
    /// What did not load or was not done, newest last (at most 16): `game_state` shows it.
    problems: Vec<String>,
    /// The problems among them that the last reading of the files said ([`Shell::use_files`]).
    file_problems: Vec<String>,
    run: Run,
    settings: Settings,
    store: SettingsStore,
    settings_dirty: bool,
    /// What the player's own settings file holds: the choices the player made, and nothing
    /// else (`specula_ludus::Kept`). An agent's change is the session's (`settings`) and never
    /// reaches it; nor does a default: a game file an agent rewrote changes what the session
    /// uses, and the player's choices stay as the player made them (reviews R1 and R2 of M6
    /// part 3: first the whole of `settings` was written at the player's next change, then a
    /// choice the changed game file no longer took was written back as its new default).
    kept: Kept,
    /// The state the screens bind to must be filled again, and the screens shown again: set
    /// by whatever changes what a screen may show.
    stale: bool,
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
    /// What the modules keep that the screens are not given, and why: the first
    /// [`MAX_UNSHOWN`], and whether there were more.
    unshown: Vec<(String, Unshown)>,
    unshown_more: bool,
    events_out: Vec<(String, Value)>,
    /// What each of the game file's `keep` names last had in its module, in their order: a
    /// value is written to the player's file when it differs from this, and not looked at
    /// otherwise (nothing is copied a tick).
    kept_seen: Vec<Option<Value>>,
    events_dropped: u64,
    clock_wish: Option<bool>,
    requests: Vec<GameRequest>,
    quit: bool,
    /// A headless run by a script (`specula run`): the script stands in for the player, so its
    /// press of Quit is the player's, and the rules' `quit` ends the run ([`Game::played_by_script`]).
    script_player: bool,
    /// The rules asked to quit where nothing ends by it (a window, a served engine): said once.
    rules_quit_said: bool,
    refused: u32,
    refused_told: u32,
    title_sent: bool,
    /// The view the window was last given from the game file (`view`): given again when the
    /// file's view changes, and whenever nobody else has the window's camera.
    view_given: Option<specula_ludus::game::View>,
    audio: Option<Arc<crate::audio_service::AudioService>>,
    ui_events: UiEvents,
}

/// The value a key names, for a condition: a module's (`gate.hp`), a setting's
/// (`setting.difficulty`), the run's own (`game.runs`, `game.phase`).
fn value_of(shown: &[(String, Shown)], settings: &Settings, runs: u64, phase: Phase, key: &str) -> Option<Val> {
    if let Some(name) = key.strip_prefix("setting.") {
        return settings.get(name).cloned();
    }
    match key {
        "game.runs" => return Some(Val::Num(runs as f64)),
        "game.phase" => return Some(Val::Text(phase.name().to_owned())),
        _ => {}
    }
    shown.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.val())
}

/// Whether a key's value says "yes", as `show` and `quit` read it: true, a number that is not 0,
/// text that is not empty.
fn truthy(value: Option<Val>) -> bool {
    match value {
        Some(Val::Bool(b)) => b,
        Some(Val::Num(n)) => n != 0.0,
        Some(Val::Text(t)) => !t.is_empty(),
        None => false,
    }
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
        Shell::build(name, ShellFiles::read(project, assets))
    }

    /// The shell made from `files`.
    pub fn build(name: &str, files: ShellFiles) -> Result<Shell, String> {
        let game = GameFile::parse(&files.game).map_err(|p| format!("{}: {p}", specula_ludus::game::FILE))?;
        let mut map = ActionMap::new();
        specula_ui::add_ui_actions(&mut map).map_err(|e| format!("the interface's own actions: {e}"))?;
        let mut shell = Shell {
            name: name.to_owned(),
            run: Run::new(&game),
            settings: Settings::new(&game.settings, std::slice::from_ref(&game.locale)),
            kept: Kept::default(),
            game,
            problems: Vec::new(),
            file_problems: Vec::new(),
            store: SettingsStore::default(),
            settings_dirty: false,
            stale: true,
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
            unshown: Vec::new(),
            unshown_more: false,
            events_out: Vec::new(),
            kept_seen: Vec::new(),
            events_dropped: 0,
            clock_wish: None,
            requests: Vec::new(),
            quit: false,
            script_player: false,
            rules_quit_said: false,
            refused: 0,
            refused_told: 0,
            title_sent: false,
            view_given: None,
            audio: None,
            ui_events: UiEvents::default(),
            files: ShellFiles::default(),
        };
        shell.use_files(files);
        Ok(shell)
    }

    /// Takes the files as read: tables, then the interface. What does not load is noted and
    /// the rest goes on (a screen with a typo does not take the game's menus away).
    fn use_files(&mut self, files: ShellFiles) {
        // What the files' last reading said is said again only if it is still so: a problem
        // of a file that was put right goes, and "N files" is said once, as it stands now
        // (review R3 of M6 part 3: every write of a table added its own count, and sixteen of
        // them pushed every other problem out).
        let before = std::mem::take(&mut self.file_problems);
        self.problems.retain(|p| !before.contains(p));
        let others = self.problems.clone();
        for problem in &files.problems {
            note(&mut self.problems, problem.clone());
        }
        self.files = files;
        self.load_tables();
        self.load_face();
        self.apply_language();
        self.laid_out = None;
        self.stale = true;
        self.file_problems = self.problems.iter().filter(|p| !others.contains(p)).cloned().collect();
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
        // The settings again from the game file's declarations and these languages: the
        // player's own choices where they fit what the game takes now, then what the session
        // held where that still fits (an agent's change this session, or the same choices).
        // The player's choices themselves are not touched: one that does not fit now is not
        // used, and is still theirs.
        let language = self.language_setting();
        let chosen = self.settings.locale().to_owned();
        let mut settings = Settings::new(&self.game.settings, &locales);
        settings.read_text(&self.kept.to_text());
        for def in self.settings.defs() {
            if let Some(value) = self.settings.get(&def.name) {
                let _ = settings.set(&def.name, value.clone());
            }
        }
        if settings.locale() != chosen && locales.contains(&chosen) {
            let _ = settings.set_text(&language, &chosen);
        }
        self.settings = settings;
        self.tables = tables;
        self.strings = strings;
    }

    /// The name of the game's language setting.
    fn language_setting(&self) -> String {
        self.settings.defs().iter().find(|d| d.kind == specula_ludus::SettingKind::Locale).map_or_else(|| "language".to_owned(), |d| d.name.clone())
    }

    /// The project's theme (the built-in one when the project's does not load), with the
    /// player's text size: the theme's own `text_scale` times the setting that stands for it.
    fn theme(&mut self) -> Result<Theme, String> {
        let text_size = self.scale_of("text_size");
        let sized = |text: &str| -> Result<Theme, String> {
            if (text_size - 1.0).abs() < 1e-4 {
                return Theme::from_json_str(text).map_err(|e| e.to_string());
            }
            let mut json = Json::parse(text).map_err(|e| e.to_string())?;
            let own = json.get("text_scale").and_then(Json::as_f64).unwrap_or(1.0);
            json.set("text_scale", Json::Num((own * f64::from(text_size)).clamp(0.5, 3.0)));
            Theme::from_json_str(&json.to_string()).map_err(|e| e.to_string())
        };
        match sized(&self.files.theme) {
            Ok(theme) => Ok(theme),
            Err(e) => {
                note(&mut self.problems, format!("assets/ui/theme.json: {e} (the built-in theme is used)"));
                sized(builtin("assets/ui/theme.json")).map_err(|e| format!("the built-in theme: {e}"))
            }
        }
    }

    /// The player's text size, shown: the theme again, in the window's interface and a
    /// capture's (their screens, focus and open menus stay as they are).
    fn apply_text_size(&mut self) {
        let theme = match self.theme() {
            Ok(theme) => theme,
            Err(e) => return note(&mut self.problems, format!("the text size: {e}")),
        };
        let mut failed = None;
        if let Some(face) = &mut self.face {
            if let Err(e) = face.ui.set_theme(theme.clone()) {
                failed = Some(e.to_string());
            }
            if let Some((ui, _)) = &mut face.capture {
                let _ = ui.set_theme(theme);
            }
        }
        if let Some(e) = failed {
            note(&mut self.problems, format!("the text size: {e}"));
        }
        self.laid_out = None;
        self.stale = true;
    }

    fn load_ui(&mut self) -> Result<(Ui, Vec<String>), String> {
        let face = |b: specula_text::Bundled| b.load().map_err(|e| format!("bundled font {}: {e}", b.name()));
        let fonts = FontSet::new().with("ui", face(specula_text::Bundled::UiRegular)?).with("ui_bold", face(specula_text::Bundled::UiBold)?).with("title", face(specula_text::Bundled::Display)?);
        let theme = self.theme()?;
        let mut ui = Ui::new(theme, fonts);
        // A game's menus say which button is chosen from their first frame (three judges of the
        // comparison's second pass saw a title whose chosen Start looked like any other button).
        ui.show_focus();
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
            Ok((ui, screens)) => {
                let layer = |text: &str| Json::parse(text).ok().and_then(|j| j.get("layer").and_then(Json::as_f64)).unwrap_or(0.0);
                let layers = self.files.screens.iter().filter(|(stem, _)| screens.contains(stem)).map(|(stem, text)| (stem.clone(), layer(text))).collect();
                self.face = Some(Face { ui, overlay: GpuOverlay::new(), capture: None, screens, layers });
            }
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
        self.stale = true;
    }

    /// What a setting stands for in the engine, done: the language, the volume.
    fn apply_setting(&mut self, name: &str) {
        let Some(def) = self.settings.def(name).cloned() else { return };
        if def.kind == specula_ludus::SettingKind::Locale {
            self.apply_language();
        }
        match def.is.as_deref() {
            Some("volume") => self.apply_volume(),
            Some("text_size") => self.apply_text_size(),
            // (The interface's scale is read at every layout.)
            _ => {}
        }
    }

    /// The game file changed while the game runs (`text`: its new text). A file that is not a
    /// game file changes nothing and is said; otherwise its title, its screens' parts, its
    /// outcome and its settings are the new ones, the player's choices kept where the setting
    /// still exists. Where the run stands is kept. The tick rate is the clock's for the run:
    /// a new one is used from the next start.
    fn reload_game(&mut self, text: &str) {
        let game = match GameFile::parse(text) {
            Ok(game) => game,
            Err(p) => return note(&mut self.problems, format!("{}: {p} (the game goes on with the file as it was)", specula_ludus::game::FILE)),
        };
        if game == self.game {
            return;
        }
        if game.tick_hz != self.game.tick_hz {
            note(&mut self.problems, format!("{}: tick_hz {} is used from the next start (this run's clock ticks at {})", specula_ludus::game::FILE, game.tick_hz, self.game.tick_hz));
        }
        // The clock keeps this run's rate, and game_state says the rate that is ticking.
        let tick_hz = self.game.tick_hz;
        self.game = GameFile { tick_hz, ..game };
        self.files.game = text.to_owned();
        self.run.reconfigure(&self.game);
        // The settings again from the new declarations (the string tables are as they were).
        let text_size = self.scale_of("text_size");
        self.load_tables();
        self.apply_language();
        self.apply_volume();
        if (self.scale_of("text_size") - text_size).abs() > 1e-4 {
            self.apply_text_size();
        }
        self.title_sent = false;
        self.laid_out = None;
        self.stale = true;
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

    /// Does what the run answered. `player`: the press was the person's own at the window.
    fn apply(&mut self, effects: Vec<Effect>, player: bool) {
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
                    // The player's own file keeps the player's own changes, by who pressed: an
                    // agent's change is the session's, kept only where the operator named the
                    // settings folder (scripts, tests).
                    if player || self.store.agents_save {
                        if let Some(value) = self.settings.get(&name).cloned() {
                            self.kept.set(&name, &value);
                        }
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
        self.stale = true;
    }

    /// A button's word, done. `Err`: what was refused, and why.
    pub fn press(&mut self, word: &str, player: bool) -> Result<(), String> {
        self.press_from(word, None, player).map_err(|(code, message)| format!("{message} ({code})"))
    }

    /// A button's word, done. `from`: the button's id and its screen, which a word that goes
    /// to the game (the event `ui`) carries with it, so one module can tell two buttons that
    /// say the same word apart. `Err`: a code to branch on, and why.
    fn press_from(&mut self, word: &str, from: Option<(&str, &str)>, player: bool) -> Result<(), (&'static str, String)> {
        let mut action = Action::parse(word).map_err(|why| ("bad_action", why))?;
        // On a screen the rules put up, the words that move the run and the screens are the
        // rules': `start`, `pause`, `resume`, `title`, `back`, `close` reach them as the event
        // `ui`, as any word of the game's own does, and the engine does nothing. A pause
        // screen of the rules' own with a button that says `resume` was answered by the
        // engine, which had paused nothing, and the rules never heard it (five of fifteen
        // games of 2026-11-23). What serves any menu stays the engine's there too: settings
        // (`set:`, `step:`, `cycle:`), `open:SCREEN`, `save:`, `load:`, `quit`.
        let rules_own = from.is_some_and(|(_, screen)| self.game.show.iter().any(|(shown, _)| shown == screen));
        if rules_own && matches!(action, Action::Start | Action::Pause | Action::Resume | Action::Title | Action::Back | Action::Close) {
            action = Action::Game(word.to_owned());
        }
        let screens: Vec<String> = self.face.as_ref().map(|f| f.screens.clone()).unwrap_or_default();
        let has = |name: &str| screens.iter().any(|s| s == name);
        match self.run.act(&action, &mut self.settings, player, &has) {
            Ok(mut effects) => {
                if let Some((widget, screen)) = from {
                    for effect in &mut effects {
                        if let Effect::Event { name: "ui", data } = effect {
                            data.push(("widget", Val::Text(widget.to_owned())));
                            data.push(("screen", Val::Text(screen.to_owned())));
                        }
                    }
                }
                self.apply(effects, player);
                Ok(())
            }
            Err(refused) => {
                if refused.code == "players_only" {
                    self.refused += 1;
                }
                Err((refused.code, refused.message))
            }
        }
    }

    /// What the modules that are running keep, taken as screens and conditions read it. Kept
    /// in place: an entry that has not changed costs a comparison, and nothing is marked for
    /// the screens to be filled again.
    fn see(&mut self, modules: &ModuleStates) {
        self.see_each(modules.each());
    }

    /// [`Shell::see`], from each module's name and what it keeps. A value the screens are not
    /// given is listed with why ([`Unshown`]), kept in place as what is shown is.
    fn see_each<'m>(&mut self, each: impl Iterator<Item = (&'m str, &'m BTreeMap<String, Value>)>) {
        let (mut n, mut m) = (0, 0);
        let mut more = false;
        'all: for (module, store) in each {
            for (key, value) in store {
                let past = n >= MAX_SHOWN;
                let shown = if past {
                    None
                } else {
                    let same_key = self.shown.get(n).is_some_and(|(k, _)| is_key(k, module, key));
                    // As it was: no copy is made of it.
                    if same_key && self.shown[n].1.is(value) {
                        n += 1;
                        continue;
                    }
                    Shown::of(value)
                };
                let Some(shown) = shown else {
                    if m == MAX_UNSHOWN {
                        more = true;
                        if past {
                            break 'all;
                        }
                        continue;
                    }
                    let why = if past { Unshown::Past } else { Unshown::of(value) };
                    match self.unshown.get_mut(m) {
                        Some((k, w)) if is_key(k, module, key) => *w = why,
                        Some(entry) => *entry = (format!("{module}.{key}"), why),
                        None => self.unshown.push((format!("{module}.{key}"), why)),
                    }
                    m += 1;
                    continue;
                };
                let same_key = self.shown.get(n).is_some_and(|(k, _)| is_key(k, module, key));
                if same_key {
                    if self.shown[n].1 != shown {
                        self.shown[n].1 = shown;
                        self.stale = true;
                    }
                } else {
                    let entry = (format!("{module}.{key}"), shown);
                    if n < self.shown.len() {
                        self.shown[n] = entry;
                    } else {
                        self.shown.push(entry);
                    }
                    self.stale = true;
                }
                n += 1;
            }
        }
        if self.shown.len() != n {
            self.shown.truncate(n);
            self.stale = true;
        }
        self.unshown.truncate(m);
        self.unshown_more = more;
    }

    /// The value a key names, for a condition: a module's (`gate.hp`), a setting's
    /// (`setting.difficulty`), the run's own (`game.runs`).
    fn value(&self, key: &str) -> Option<Val> {
        value_of(&self.shown, &self.settings, self.run.runs(), self.run.phase(), key)
    }

    /// The score, when the game file names the key that holds it and it holds a number.
    fn score(&self) -> Option<f64> {
        match self.value(self.game.outcome.score.as_deref()?)? {
            Val::Num(n) => Some(n),
            _ => None,
        }
    }

    /// The state filled and the screens shown, when something they show has changed since the
    /// last time (a frame of a game that stands still fills nothing).
    fn refresh(&mut self) {
        if self.stale {
            self.fill();
            self.sync_screens();
            self.stale = false;
        }
    }

    /// Fills the state the screens bind to: the run's own, the settings, what the modules keep.
    fn fill(&mut self) {
        let score = self.score();
        let language = self.language_setting();
        let screen = self.up().last().cloned().unwrap_or_default();
        let s = &mut self.state;
        s.reset();
        // The player's language's lines; where it has no table of its own, the first's, as
        // the screens' own words are.
        let strings = self.strings.iter().find(|(locale, _)| locale == self.settings.locale()).or_else(|| self.strings.first()).map(|(_, strings)| strings.clone());
        // The game's name: the game file's title, or the line `title.name` of the strings
        // where they have one, in the player's language (a name is a word too: one game of
        // the comparison's second pass failed "every word changes with the language" on it).
        let title = if self.game.title.is_empty() { self.name.as_str() } else { self.game.title.as_str() };
        s.set_str("game.title", strings.as_ref().and_then(|strings| strings.catalog().text("title.name", None)).unwrap_or(title));
        s.set_str("game.phase", self.run.phase().name());
        s.set_str(
            "game.outcome",
            match self.run.phase() {
                Phase::Ended(how) => how.name(),
                _ => Ended::Running.name(),
            },
        );
        s.set_str("game.screen", &screen);
        s.set_bool("game.paused", self.run.phase() == Phase::Paused);
        s.set_bool("game.running", self.run.phase() == Phase::Running);
        s.set_int("game.runs", self.run.runs() as i64);
        s.set_bool("game.has_score", score.is_some());
        if let Some(score) = score {
            s.set_num("game.score", score);
        }
        // A setting's value, and the value as words (`.text`): a language by its own name
        // ("Deutsch", as its table gives it), a toggle as on or off, a choice by the line
        // `setting.NAME.VALUE` of the player's language when the tables have one (a choice's
        // values are words too: "easy" is "Facile" to a player who reads French), anything
        // else as it is.
        for def in self.settings.defs() {
            let (key, words) = (format!("setting.{}", def.name), format!("setting.{}.text", def.name));
            match self.settings.get(&def.name) {
                Some(Val::Bool(b)) => {
                    s.set_bool(&key, *b);
                    s.set_str(&words, if *b { "on" } else { "off" });
                }
                Some(Val::Num(n)) => {
                    s.set_int(&key, *n as i64);
                    s.set_str(&words, &format!("{}", *n as i64));
                }
                Some(Val::Text(t)) => {
                    s.set_str(&key, t);
                    let line = format!("setting.{}.{t}", def.name);
                    let name = if def.name == language {
                        self.tables.iter().find(|table| table.locale.as_str() == t).map_or(t.as_str(), |table| table.name.as_str())
                    } else {
                        strings.as_ref().and_then(|strings| strings.catalog().text(&line, None)).unwrap_or(t.as_str())
                    };
                    s.set_str(&words, name);
                }
                None => {}
            }
        }
        // What the game keeps for the player, as the player's file has it: `kept.NAME`. A
        // screen shows it from the first frame, before any rule has ticked and heard it (a
        // title's "Best: 1,540", a Continue that is there only with a run to go on with).
        for (name, _) in &self.game.keep {
            let key = format!("kept.{name}");
            match self.kept.kept(name) {
                Some(Json::Bool(b)) => {
                    s.set_bool(&key, *b);
                }
                Some(Json::Num(n)) if specula_core::dmath::fract_f64(*n) == 0.0 && n.abs() < 9.0e15 => {
                    s.set_int(&key, *n as i64);
                }
                Some(Json::Num(n)) => {
                    s.set_num(&key, *n);
                }
                Some(Json::Str(t)) => {
                    s.set_str(&key, t);
                }
                // (A list or a map is there: true, for a condition.)
                Some(Json::Arr(_) | Json::Obj(_)) => {
                    s.set_bool(&key, true);
                }
                Some(Json::Null) | None => {}
            }
        }
        for (key, shown) in &self.shown {
            shown.put(key, s);
        }
    }

    /// Shows the screens the run says are up, and hides the rest.
    fn sync_screens(&mut self) {
        let up = self.up();
        let Some(face) = &mut self.face else { return };
        for name in &face.screens {
            let want = up.iter().any(|u| u == name);
            if face.ui.is_visible(name) != want {
                let _ = if want { face.ui.show(name) } else { face.ui.hide(name) };
            }
        }
        // A screen opened over another is drawn by its layer, not by the order it was opened
        // in: one whose layer is not above the screen under it is drawn underneath and never
        // gets the focus. Said, with what to change (found in the first captures of a new
        // project: its settings opened under its title).
        let layered: Vec<(&String, f64)> = up.iter().filter_map(|name| face.layers.get(name).map(|layer| (name, *layer))).collect();
        let under = layered.windows(2).find(|pair| pair[1].1 <= pair[0].1).map(|pair| (pair[0].0.clone(), pair[0].1, pair[1].0.clone(), pair[1].1));
        if let Some((below, below_layer, above, above_layer)) = under {
            note(&mut self.problems, format!("assets/ui/{above}.json opens over {below} with \"layer\": {above_layer}, which is not above {below}'s {below_layer}: it is drawn underneath. Give it a higher layer"));
        }
    }

    fn viewport(ui: &Ui, display: &Display, size: (u32, u32), user_scale: f32) -> specula_ui::Viewport {
        Display { size: [size.0.max(1), size.1.max(1)], ..*display }.viewport(ui.theme(), user_scale)
    }

    /// A setting that stands for a scale (`"is": "ui_scale"` or `"text_size"`) holds per cent:
    /// 100 is as designed. No such setting: as designed.
    fn scale_of(&self, meaning: &str) -> f32 {
        let value = self.settings.defs().iter().find(|d| d.is.as_deref() == Some(meaning)).and_then(|d| self.settings.get(&d.name));
        match value {
            Some(Val::Num(n)) => (*n as f32 / 100.0).clamp(0.5, 3.0),
            _ => 1.0,
        }
    }

    /// The presses of one laid-out frame, done.
    fn handle(&mut self, events: &UiEvents, player: bool) {
        let mut backed = false;
        for event in &events.events {
            match event {
                UiEvent::Action { action, widget } => {
                    backed |= matches!(&**action, "back" | "resume" | "close");
                    let from = self.face.as_ref().map(|f| (f.ui.widget_id(*widget).unwrap_or("").to_owned(), f.ui.screen_name(widget.screen).to_owned()));
                    let from = from.as_ref().map(|(widget, screen)| (widget.as_str(), screen.as_str()));
                    // (A script that stands in for the player presses Quit as the player would.)
                    let player = self.as_player(action, player);
                    if let Err((_, why)) = self.press_from(action, from, player) {
                        self.say(&why, ToastKind::Warn);
                    }
                }
                // A menu's "back" with no action of its own. Over a screen the rules put up it
                // is the rules' to answer: the screen is theirs to take down.
                UiEvent::Back { .. } if !backed => {
                    backed = true;
                    if !self.back_to_rules() {
                        let _ = self.press("back", player);
                    }
                }
                UiEvent::Back { .. } | UiEvent::Refused { .. } => {}
            }
        }
        // Back while only the game's own screen is up (no menu took it): the pause menu.
        if !backed && !events.consumed.navigation && self.input.pressed(PAUSE) && self.run.phase() == Phase::Running && self.run.stack().is_empty() && !self.back_to_rules() {
            let _ = self.press("pause", player);
        }
    }

    /// Left and Right on a button that moves a setting step it down and up, and stop at its
    /// ends (a volume, a difficulty, a language), as Enter moves it on. Only while the press
    /// moved the focus nowhere: in a row of buttons Left and Right are the way along the row.
    /// (The measured comparison's builders looked for this and found Enter alone:
    /// docs/design/LEVERAGE.md, "Results (2026-11-16)".)
    fn step_focused_setting(&mut self, before: Option<specula_ui::WidgetRef>, player: bool) {
        let Some(face) = &self.face else { return };
        let Some(at) = face.ui.focused().filter(|now| Some(*now) == before) else { return };
        let name = match face.ui.widget_action(at).map(Action::parse) {
            Some(Ok(Action::Cycle(name) | Action::Step { name, .. } | Action::Set { name, .. })) => name,
            _ => return,
        };
        let from = (face.ui.widget_id(at).unwrap_or("").to_owned(), face.ui.screen_name(at.screen).to_owned());
        for (key, way) in [("ui_left", "-1"), ("ui_right", "+1")] {
            if self.input.pressed(key) {
                if let Err((_, why)) = self.press_from(&format!("step:{name}:{way}"), Some((from.0.as_str(), from.1.as_str())), player) {
                    self.say(&why, ToastKind::Warn);
                }
            }
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
        let scale = self.scale_of("ui_scale");
        let Some(Face { ui: window, capture: Some((ui, overlay)), screens, .. }) = &mut self.face else { return };
        for name in screens.iter() {
            let open = window.is_visible(name);
            if ui.is_visible(name) != open {
                let _ = if open { ui.show(name) } else { ui.hide(name) };
            }
        }
        let view = Shell::viewport(ui, &self.display, out.size, scale);
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
            ("screens", Json::arr(self.up().iter().filter(|s| self.has_screen(s)).map(|s| Json::from(s.as_str())))),
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
        // What the modules keep that no screen is given, and why: a screen that binds one shows
        // its key as written, and a run says why from this.
        if !self.unshown.is_empty() {
            j.set("not_shown", Json::arr(self.unshown.iter().map(|(key, why)| Json::obj([("key", Json::from(key.as_str())), ("why", Json::from(why.words()))]))));
            if self.unshown_more {
                j.set("not_shown_more", Json::Bool(true));
            }
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
        // What the game kept for the player is told first: the rules have it before they hear
        // that a run begins.
        self.tell_kept();
        let effects = self.run.began(&self.settings);
        self.apply(effects, false);
    }

    fn after_modules(&mut self, ctx: &mut Ctx, modules: &ModuleStates) {
        self.see(modules);
        self.keep_values(modules);
        // (The run is asked with what it reads lent beside it: nothing is copied a tick.)
        let (runs, phase) = (self.run.runs(), self.run.phase());
        let effects = {
            let Shell { run, shown, settings, .. } = self;
            let look = |key: &str| value_of(shown, settings, runs, phase, key);
            run.after_tick(&look, ctx.clock.tick())
        };
        if !effects.is_empty() {
            self.apply(effects, false);
        }
        self.rules_quit();
    }

    fn played_by_script(&mut self) {
        self.script_player = true;
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
        self.stale = true;
    }

    fn game_state(&self) -> Option<Json> {
        Some(self.state_json())
    }

    fn modules(&self) -> Vec<String> {
        self.game.modules.clone()
    }

    fn volume(&self) -> Option<f32> {
        self.settings.fraction("volume").map(|v| v as f32)
    }

    fn told(&mut self, problem: &str) {
        note(&mut self.problems, problem.to_owned());
        self.say(problem, ToastKind::Warn);
    }

    fn act(&mut self, _ctx: &mut Ctx, word: &str) -> Option<Result<(), (&'static str, String)>> {
        // An agent's press: what only the player may do is refused (and counted).
        Some(self.press_from(word, None, false))
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
        // The window looks at the game as its file says (`view`), unless someone has pointed
        // it elsewhere (an agent's window_view, the editor): a 2D game is seen straight on,
        // as its headless frame is, not by the camera that fits whatever is in the world.
        if !frame.synthetic && (frame.camera.is_none() || self.view_given != self.game.view) {
            if let Some(view) = self.game.view {
                use specula_core::math::Vec3;
                use specula_ludus::game::View;
                *frame.camera = Some(match view {
                    View::Flat { center, height } => crate::sight::Camera::view_2d(center[0] as f32, center[1] as f32, height as f32),
                    View::Eye { eye, target, fov_y_deg } => crate::sight::Camera::new(Vec3::new(eye[0] as f32, eye[1] as f32, eye[2] as f32), Vec3::new(target[0] as f32, target[1] as f32, target[2] as f32), fov_y_deg as f32),
                });
            }
            self.view_given = self.game.view;
        }
        if !frame.synthetic && !self.title_sent {
            *frame.title = Some(if self.game.title.is_empty() { self.name.clone() } else { self.game.title.clone() });
            self.title_sent = true;
        }
        self.idle.set_last_device(self.input.last_device());
        self.idle.set_pad_style(self.input.pad_style());
        if frame.size.0 == 0 || frame.size.1 == 0 {
            return;
        }
        self.refresh();
        let mut events = std::mem::take(&mut self.ui_events);
        events.events.clear();
        events.consumed = Default::default();
        let scale = self.scale_of("ui_scale");
        let mut focused_before = None;
        if let Some(face) = &mut self.face {
            let view = Shell::viewport(&face.ui, &self.display, frame.size, scale);
            // (Copied field by field: reusing the buffer, and no wholesale replacement by name.)
            focused_before = face.ui.focused();
            let got = face.ui.frame(&self.input, &self.state, &view).events;
            events.events.extend(got.events.iter().cloned());
            events.consumed = got.consumed;
            self.laid_out = Some(frame.size);
        }
        self.handle(&events, !frame.synthetic);
        self.step_focused_setting(focused_before, !frame.synthetic);
        self.ui_events = events;
    }

    fn overlay(&mut self, _ctx: &mut Ctx, out: &mut Overlay) {
        self.refresh();
        if out.capture {
            self.capture_overlay(out);
            return;
        }
        let scale = self.scale_of("ui_scale");
        let Some(face) = &mut self.face else { return };
        if self.laid_out != Some(out.size) {
            let view = Shell::viewport(&face.ui, &self.display, out.size, scale);
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
            self.refresh();
        }
        let user_scale = self.scale_of("ui_scale");
        let face = self.face.as_mut()?;
        let ui = match (pick, &mut face.capture) {
            (UiPick::Capture, Some((ui, _))) => ui,
            _ => &mut face.ui,
        };
        Some(GameUi { ui, state: &self.state, user_scale, input_map: Some(self.input.map()) })
    }

    fn use_settings(&mut self, text: Option<&str>, store: &SettingsStore) {
        let text_size = self.scale_of("text_size");
        self.store = store.clone();
        // A file that is there and is no settings file (damaged, another format) holds nothing
        // that can be read, and is not written over: what the game kept for the player in it
        // may still be put right by hand.
        let text = text.filter(|text| {
            let is = Kept::is_file(text);
            if !is {
                let why = "the settings file is not a settings file (damaged, or another format): left as it is, defaults used, nothing saved over it";
                self.store.problem = Some(why.to_owned());
                note(&mut self.problems, format!("the saved settings: {why}"));
            }
            is
        });
        if let Some(text) = text {
            for problem in self.settings.read_text(text) {
                note(&mut self.problems, format!("the saved settings: {problem}"));
            }
            // (The player's own file: the layer that is written back holds it too.)
            self.kept = Kept::read_text(text);
        }
        self.apply_language();
        self.apply_volume();
        if (self.scale_of("text_size") - text_size).abs() > 1e-4 {
            self.apply_text_size();
        }
    }

    fn take_settings(&mut self) -> Option<String> {
        let dirty = std::mem::take(&mut self.settings_dirty);
        (dirty && self.store.saving()).then(|| self.kept.to_text())
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

    fn asset_texts(&mut self, _ctx: &mut Ctx, texts: &[(String, Arc<str>)]) {
        // The screens, the theme and the words as the project store holds them now: nothing is
        // read from the disk here (the engine thread).
        let game = std::mem::take(&mut self.files.game);
        self.use_files(ShellFiles::of_texts(game, texts));
    }

    fn game_file_changed(&mut self, _ctx: &mut Ctx, text: &str) {
        self.reload_game(text);
    }

    fn save_state(&self) -> Option<Result<Vec<u8>, String>> {
        let mut j = Json::obj([("format", Json::from(SAVE_FORMAT)), ("phase", Json::from(self.run.phase().name())), ("runs", Json::from(self.run.runs()))]);
        if let Some(tick) = self.run.ended_at() {
            j.set("ended_at", Json::from(tick));
        }
        // What the modules have not been told yet (a run that began, a setting, a button's
        // word: told at the next tick). It is part of where the game stands: a save made
        // between the telling and the tick lost it, and the game loaded was a game whose
        // rules never heard that their run began (found by `specula find`, 2026-11-22: a
        // script it found from a save of the start did not win in a run of its own). Each
        // is its name and its data as the module host encodes a value, so that a number
        // comes back as the kind of number it was.
        if !self.events_out.is_empty() {
            j.set("events", Json::arr(self.events_out.iter().map(|(name, data)| Json::arr([Json::from(name.as_str()), Json::from(specula_core::base64::encode(&data.encode()).as_str())]))));
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
        // (A save from before runs were kept has none: one run, if it was ever under way.)
        let runs = j.get("runs").and_then(Json::as_u64).unwrap_or(u64::from(phase != Phase::Title));
        // (A save from before the waiting events were kept has none.)
        let mut events: Vec<(String, Value)> = Vec::new();
        for event in j.get("events").and_then(Json::as_array).unwrap_or(&[]).iter().take(MAX_EVENTS) {
            let pair = event.as_array().filter(|p| p.len() == 2);
            let (Some(name), Some(data)) = (pair.and_then(|p| p[0].as_str()), pair.and_then(|p| p[1].as_str())) else {
                return Err("the game's part of the save has an event that is not a name and its data".to_owned());
            };
            let data = specula_core::base64::decode(data).ok().and_then(|bytes| Value::decode(&bytes).ok()).ok_or_else(|| format!("the game's part of the save has an event ('{name}') whose data cannot be read"))?;
            events.push((name.to_owned(), data));
        }
        Ok(Box::new((phase, j.get("ended_at").and_then(Json::as_u64), runs, events)))
    }

    fn apply_state(&mut self, _: &mut Ctx, state: Box<dyn std::any::Any>) {
        self.put_back(state);
    }
}

impl Shell {
    /// Whether a press of `action` from a frame is the player's: the window's own input is
    /// (`player`); under a script (`specula run`), which stands in for the player, its press of
    /// Quit is too. Nothing else of the player's alone is widened, and an agent's `game_act`
    /// never comes through here.
    fn as_player(&self, action: &str, player: bool) -> bool {
        player || (self.script_player && action == "quit")
    }

    /// The rules' own Quit (the game file's `quit`), after a tick: it ends a run by a script;
    /// anywhere else it is said once and ends nothing (an agent can drive the rules, and agents
    /// never close a person's window: there a button whose action is `quit` does).
    fn rules_quit(&mut self) {
        let Some(key) = &self.game.quit else { return };
        if !truthy(self.value(key)) {
            return;
        }
        if self.script_player {
            self.quit = true;
        } else if !self.rules_quit_said {
            self.rules_quit_said = true;
            note(&mut self.problems, format!("the rules asked to quit ({key}, the game file's \"quit\"): that ends a headless run (specula run) there; in a window the game ends by a button whose action is quit"));
        }
    }

    /// Escape (or a pad's back) while a screen the rules put up is on top: told to the rules as
    /// the event `ui` with the action `back` and the screen's name, and nothing else is done.
    /// The game is not paused over the rules' own screen, and the screen is not closed: the
    /// rules take it down by its key, or do nothing (a shop Escape does nothing in). Whether
    /// such a screen was on top. (A second reader, 2026-11-23: four of five games paused in
    /// their shop, against their brief, and the guide gave them no way out.)
    fn back_to_rules(&mut self) -> bool {
        if self.run.phase() != Phase::Running || !self.run.stack().is_empty() {
            return false;
        }
        let Some(top) = self.up().pop().filter(|top| self.game.show.iter().any(|(screen, _)| screen == top)) else { return false };
        if self.events_out.len() < MAX_EVENTS {
            self.events_out.push(("ui".to_owned(), Value::map().with("action", "back").with("screen", top.as_str())));
        } else {
            self.events_dropped += 1;
        }
        true
    }
}

impl Shell {
    /// The screens that are up, the bottom first: the phase's own, then the game's own that
    /// its rules put up (the game file's `show`, while a run is under way or paused), then
    /// the ones opened over them (a pause menu, the settings).
    fn up(&self) -> Vec<String> {
        let own = self.run.visible();
        let opened = self.run.stack().len();
        let (below, above) = own.split_at(own.len() - opened);
        let mut up: Vec<String> = below.iter().map(|s| (*s).to_owned()).collect();
        if matches!(self.run.phase(), Phase::Running | Phase::Paused) {
            let (runs, phase) = (self.run.runs(), self.run.phase());
            // (Under a pause menu too: the pause is over what was up.)
            let at = if phase == Phase::Paused { up.len().saturating_sub(1) } else { up.len() };
            let mut shown: Vec<String> = Vec::new();
            for (screen, key) in &self.game.show {
                let on = truthy(value_of(&self.shown, &self.settings, runs, phase, key));
                if on && !up.contains(screen) && !shown.contains(screen) {
                    shown.push(screen.clone());
                }
            }
            up.splice(at..at, shown);
        }
        up.extend(above.iter().map(|s| (*s).to_owned()));
        up
    }
}

impl Shell {
    /// Tells the modules each value the game keeps for the player and the player's file has
    /// (the game file's `keep`): an event `kept` from the engine, with `name` and `value`, at
    /// the next tick. Once a name: a save that held the telling is not told twice.
    fn tell_kept(&mut self) {
        for (name, _) in &self.game.keep {
            let Some(value) = self.kept.kept(name) else { continue };
            // (A value larger than one the game may keep was not written by it: a file changed
            // by hand. It is not told.)
            if value.to_string().len() > MAX_KEPT_BYTES {
                note(&mut self.problems, format!("\"keep\": '{name}' in the settings file is larger than a kept value ({MAX_KEPT_BYTES} bytes): not told to the rules"));
                continue;
            }
            let told = Value::map().with("name", name.as_str()).with("value", Value::from_json(value));
            // A telling a save brought back is of the save's day: the player's file is newer.
            let waiting = self.events_out.iter().position(|(event, data)| event == "kept" && data.get("name").and_then(Value::as_str) == Some(name.as_str()));
            match waiting {
                Some(at) => self.events_out[at].1 = told,
                None if self.events_out.len() < MAX_EVENTS => self.events_out.push(("kept".to_owned(), told)),
                None => {}
            }
        }
    }

    /// Keeps for the player what the game file's `keep` names, as the modules have it after
    /// this tick: written to the player's file (where one is kept) when a value changed.
    fn keep_values(&mut self, modules: &ModuleStates) {
        let Shell { game, kept_seen, kept, settings_dirty, problems, stale, .. } = self;
        if kept_seen.len() != game.keep.len() {
            kept_seen.resize(game.keep.len(), None);
        }
        for (i, (name, key)) in game.keep.iter().enumerate() {
            // (A module that is not running keeps nothing new: what was kept stays.)
            let Some(value) = modules.dotted(key) else { continue };
            if kept_seen[i].as_ref() == Some(value) {
                continue;
            }
            kept_seen[i] = Some(value.clone());
            let json = value.to_json();
            let size = json.to_string().len();
            if size > MAX_KEPT_BYTES {
                note(problems, format!("\"keep\": '{name}' ({key}) is {size} bytes as JSON and a kept value is at most {MAX_KEPT_BYTES}: it is not kept"));
                continue;
            }
            // All of it together stays a file that is read again (2026-11-23: sixteen values
            // at their most made a file its own reader refused).
            let whole = kept.to_text().len() + size + name.len() + 16;
            if whole > MAX_KEPT_TOTAL {
                note(problems, format!("\"keep\": with '{name}' ({size} bytes) the player's file would be {whole} bytes, and what a game keeps is {MAX_KEPT_TOTAL} at most in all: it is not kept"));
                continue;
            }
            if kept.keep(name, json) {
                *settings_dirty = true;
                // (A screen may show it: `kept.NAME`.)
                *stale = true;
            }
        }
    }
}

/// The largest value a game keeps for the player, as JSON text.
const MAX_KEPT_BYTES: usize = 4096;
/// The largest the player's file is let grow by what a game keeps: under what its reader
/// takes ([`crate::user_settings::MAX_BYTES`]), with room for the settings.
const MAX_KEPT_TOTAL: usize = 48 << 10;

impl Shell {
    /// Puts the run back as a save had it ([`Game::apply_state`]).
    fn put_back(&mut self, state: Box<dyn std::any::Any>) {
        if let Ok(saved) = state.downcast::<(Phase, Option<u64>, u64, Vec<(String, Value)>)>() {
            let saved = *saved;
            self.run.restore(saved.0, saved.1, saved.2);
            // What was pressed before the load was pressed in another run: it is not told to
            // the modules the save brought back. What the saved game's modules had still to
            // be told is.
            self.events_out = saved.3;
            // A quit asked before the load was asked in another run: the game brought back has
            // asked for none (no save holds one; a run that quit is over).
            self.quit = false;
            // And what the game keeps for the player, which no save holds: the rules a save
            // brought back have its own values, older than the player's.
            self.kept_seen.clear();
            self.tell_kept();
            // The clock is as the phase says: only a run under way ticks.
            self.clock_wish = Some(saved.0 != Phase::Running);
            self.laid_out = None;
            self.stale = true;
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

    /// What the game keeps for the player is told to the rules before a run begins, once a
    /// name; and a screen the rules put up is over the game's own and under a pause.
    #[test]
    fn kept_values_are_told_and_the_rules_screens_are_up() {
        let game = GAME.replace("\"outcome\"", "\"keep\": { \"best\": \"gate.best\", \"run\": \"gate.saved\" }, \"show\": { \"shop\": \"gate.shopping\" }, \"outcome\"");
        let mut shell = Shell::build("gate", ShellFiles::builtin(&game)).expect("a shell");
        let file = "{\"format\": \"specula.settings.v1\", \"values\": {\"volume\": 3, \"kept.best\": 40, \"kept.other\": 1}}";
        shell.use_settings(Some(file), &SettingsStore { kind: "per_user", agents_save: false, problem: None });
        shell.tell_kept();
        shell.tell_kept();
        let told = events(&mut shell);
        assert_eq!(told, vec![("kept".to_owned(), Value::map().with("name", "best").with("value", 40i64))], "the one name the file has, once");
        assert_eq!(shell.settings.get("volume"), Some(&Val::Num(3.0)), "a setting beside it is as it was");
        // What the game kept stays in the player's file when a setting is written.
        shell.press("step:volume:-1", true).unwrap();
        let text = shell.take_settings().expect("the file");
        assert!(text.contains("\"kept.best\": 40") && text.contains("\"volume\": 2"), "{text}");
        // The rules' own screen: up while a run is under way and its key is true.
        assert_eq!(shell.up(), ["title"]);
        shell.shown.push(("gate.shopping".to_owned(), Shown::Bool(true)));
        assert_eq!(shell.up(), ["title"], "not at the title");
        shell.press("start", true).unwrap();
        assert_eq!(shell.up(), ["hud", "shop"]);
        // The words that move the run are the rules' on a screen the rules put up: a Resume on
        // a pause screen of the game's own is told to them, and the engine pauses and resumes
        // nothing. What serves any menu is still done by the engine there.
        shell.press_from("resume", Some(("go", "shop")), true).unwrap();
        shell.press_from("title", Some(("out", "shop")), true).unwrap();
        let heard = events(&mut shell);
        assert_eq!(heard.iter().filter(|(name, _)| name == "ui").map(|(name, data)| (name.as_str(), data.get("action").and_then(Value::as_str), data.get("screen").and_then(Value::as_str))).collect::<Vec<_>>(), [("ui", Some("resume"), Some("shop")), ("ui", Some("title"), Some("shop"))]);
        assert_eq!((shell.up(), shell.run.phase()), (vec!["hud".to_owned(), "shop".to_owned()], Phase::Running), "the run is as it was");
        shell.press_from("open:settings", Some(("s", "shop")), true).unwrap();
        assert_eq!(shell.up(), ["hud", "shop", "settings"]);
        shell.press("back", true).unwrap();
        // On the engine's own pause screen `resume` is the engine's, as it was.
        shell.press("pause", true).unwrap();
        shell.press_from("resume", Some(("resume", "pause")), true).unwrap();
        assert_eq!(shell.run.phase(), Phase::Running);
        assert!(events(&mut shell).iter().all(|(_, data)| data.get("action").and_then(Value::as_str) != Some("resume")));
        // What was kept is on a screen from the first frame, under `kept.NAME`.
        shell.fill();
        assert_eq!(shell.state.get("kept.best").and_then(|v| v.as_f64()), Some(40.0));
        assert!(shell.state.get("kept.run").is_none(), "nothing kept under it yet");
        // Escape over the rules' own screen is the rules' to answer: told, and nothing done.
        assert!(shell.back_to_rules());
        assert_eq!(events(&mut shell).last(), Some(&("ui".to_owned(), Value::map().with("action", "back").with("screen", "shop"))));
        assert_eq!((shell.up(), shell.run.phase()), (vec!["hud".to_owned(), "shop".to_owned()], Phase::Running));
        shell.press("pause", true).unwrap();
        assert!(!shell.back_to_rules(), "under a pause it is the pause's");
        shell.press("resume", true).unwrap();
        // A telling that waits (a save's, of its own day) takes the player's file's value.
        shell.events_out = vec![("kept".to_owned(), Value::map().with("name", "best").with("value", 5i64))];
        shell.tell_kept();
        assert_eq!(events(&mut shell), vec![("kept".to_owned(), Value::map().with("name", "best").with("value", 40i64))]);
        // A file that is there and is no settings file is never written over (a second reader,
        // 2026-11-23: the first change after such a start replaced it with defaults, and the
        // file holds what the game kept for the player).
        let mut damaged = Shell::build("gate", ShellFiles::builtin(&game)).expect("a shell");
        damaged.use_settings(Some("{\"format\": \"specula.settings.v1\", \"values\": {\"kept.best\": 40"), &SettingsStore { kind: "folder", agents_save: true, problem: None });
        assert!(damaged.store.problem.as_deref().is_some_and(|p| p.contains("not a settings file")) && !damaged.store.saving());
        damaged.press("step:volume:-1", true).unwrap();
        assert_eq!(damaged.take_settings(), None);
        let unread = SettingsStore { kind: "folder", agents_save: true, problem: Some("the settings file is 70000 bytes".into()) };
        assert!(!unread.saving());
        shell.press("pause", true).unwrap();
        assert_eq!(shell.up(), ["hud", "shop", "pause"]);
        shell.press("open:settings", true).unwrap();
        assert_eq!(shell.up(), ["hud", "shop", "pause", "settings"]);
        shell.press("back", true).unwrap();
        shell.press("resume", true).unwrap();
        shell.shown[0].1 = Shown::Int(0);
        assert_eq!(shell.up(), ["hud"], "taken down by the key");
    }

    /// A choice's value is words too: shown by the line `setting.NAME.VALUE` of the player's
    /// language where the tables have one, and as it is where they do not (a builder of the
    /// comparison's change round spent ten requests getting "hard" to read "Difficile").
    #[test]
    fn a_choices_value_is_shown_in_the_players_language() {
        let mut files = ShellFiles::builtin(GAME);
        let table = |locale: &str, name: &str, source: bool, lines: &str| (locale.to_owned(), format!(r#"{{"format": "specula.strings.v1", "locale": "{locale}", "name": "{name}", "source": {source}, "strings": {{{lines}}}}}"#));
        files.tables = vec![table("en", "English", true, r#""setting.difficulty.easy": "Easy", "setting.difficulty.hard": "Hard""#), table("fr", "Français", false, r#""setting.difficulty.easy": "Facile", "title.name": "Tenez la porte""#)];
        let mut shell = Shell::build("gate", files).expect("a shell");
        let words = |shell: &mut Shell| {
            shell.fill();
            shell.state.get("setting.difficulty.text").map(|v| format!("{v:?}")).unwrap_or_default()
        };
        assert!(words(&mut shell).contains("Easy"), "{}", words(&mut shell));
        // The game's name is the game file's while the strings have no line for it (here the
        // source table has none, so a translation's line is no line: a table says in another
        // language what the source says).
        let named = |shell: &Shell| shell.state.get("game.title").map(|v| format!("{v:?}")).unwrap_or_default();
        assert!(named(&shell).contains("Hold the gate"), "{}", named(&shell));
        shell.press("set:language:fr", true).unwrap();
        assert!(words(&mut shell).contains("Facile"), "{}", words(&mut shell));
        assert!(named(&shell).contains("Hold the gate"), "{}", named(&shell));
        // With the line in the source table, each language says the name its own way.
        let mut files = ShellFiles::builtin(GAME);
        files.tables = vec![table("en", "English", true, r#""title.name": "Hold the gate!""#), table("fr", "Français", false, r#""title.name": "Tenez la porte""#)];
        let mut named_game = Shell::build("gate", files).expect("a shell");
        named_game.fill();
        assert!(named(&named_game).contains("Hold the gate!"), "{}", named(&named_game));
        named_game.press("set:language:fr", true).unwrap();
        named_game.fill();
        assert!(named(&named_game).contains("Tenez la porte"), "{}", named(&named_game));
        // A value the player's table has no line for: the source language's, as every line.
        shell.press("set:difficulty:hard", true).unwrap();
        assert!(words(&mut shell).contains("Hard"), "{}", words(&mut shell));
        assert_eq!(shell.value("setting.difficulty"), Some(Val::Text("hard".into())), "the value itself is the game file's word");
        // No line at all: the value as it is.
        let mut plain = super::tests::shell();
        assert!(words(&mut plain).contains("easy"), "{}", words(&mut plain));
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
        shell.apply(effects, false);
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
        shell.press("step:volume:-1", true).unwrap();
        let text = shell.take_settings().expect("the settings file");
        assert!(text.contains("\"volume\": 7") && text.contains("specula.settings.v1"), "{text}");
        // The agent's own change (the difficulty, set to hard above) is not in the player's file,
        // though the player changed another setting after it: the file holds the player's
        // choices alone (review R1 of M6 part 3).
        assert!(!text.contains("difficulty"), "{text}");
        assert_eq!(shell.settings.get("difficulty"), Some(&Val::Text("hard".into())), "the session still has the agent's");
        assert_eq!(shell.take_settings(), None);
        // A new shell given that file has the values back.
        let mut again = super::tests::shell();
        again.use_settings(Some(&text), &SettingsStore { kind: "per_user", agents_save: false, problem: None });
        assert_eq!(again.settings.get("volume"), Some(&Val::Num(7.0)));
        // With settings off nothing is ever handed over.
        let mut off = super::tests::shell();
        off.press("step:volume:-1", true).unwrap();
        assert_eq!(off.take_settings(), None);
    }

    /// A changed screen or table is taken from the store's texts by the rule the disk is read
    /// by: each folder's files by name, and it is said when there are more than are taken
    /// (review R2 of M6 part 3: past 128 files of both folders together, in the store's order,
    /// the screens and the source table fell away with nothing said).
    #[test]
    fn the_stores_texts_are_taken_by_name_and_what_is_left_out_is_said() {
        let text = |s: &str| -> Arc<str> { Arc::from(s) };
        // More tables than are taken, named so that the source table sorts after the others in
        // the store's order (`en-XX` before `en`), then the screens: the order the store has.
        let mut texts: Vec<(String, Arc<str>)> = (0..MAX_TABLES + 8).map(|i| (format!("strings/en-{i:03}.json"), text("{}"))).collect();
        texts.push(("strings/en.json".into(), text(builtin("assets/strings/en.json"))));
        texts.push(("ui/hud.json".into(), text("{\"marker\": \"PROBE\"}")));
        texts.push(("ui/theme.json".into(), text("{\"theme\": \"mine\"}")));
        texts.push(("ui/deep/inner.json".into(), text("{}")));
        texts.sort();
        let files = ShellFiles::of_texts("GAME".into(), &texts);
        assert_eq!(files.game, "GAME");
        // The changed screen is there however many tables there are, with the theme.
        assert!(files.screens.iter().any(|(stem, text)| stem == "hud" && text.contains("PROBE")), "the project's screen is taken");
        assert!(files.theme.contains("mine"));
        assert!(!files.screens.iter().any(|(stem, _)| stem.contains("inner")), "a file in a folder below is not a screen");
        // The tables: the first by name, as the disk is read, and it is said.
        assert_eq!(files.tables.len(), MAX_TABLES);
        assert!(files.tables.iter().any(|(stem, _)| stem == "en"), "the source table is the first by name");
        assert!(files.problems.iter().any(|p| p.contains(&format!("assets/strings holds {} files: the first {MAX_TABLES} are read", MAX_TABLES + 9))), "{:?}", files.problems);
        // What a project lacks is the built-in one.
        let bare = ShellFiles::of_texts(String::new(), &[]);
        assert!(bare.screens.iter().any(|(stem, _)| stem == "title") && !bare.tables.is_empty() && !bare.theme.is_empty() && bare.problems.is_empty());
    }

    /// The files' problems are of the files as they are now: said once, and gone when the file
    /// is put right; what was noted of anything else stays (review R3 of M6 part 3).
    #[test]
    fn a_files_problem_is_said_as_it_stands_now() {
        let mut shell = shell();
        note(&mut shell.problems, "something else".to_owned());
        let game = shell.files.game.clone();
        let tables = |n: usize| -> Vec<(String, Arc<str>)> {
            // (Each a table of a locale of its own: the English one for a numbered region.)
            let other = |i: usize| builtin("assets/strings/en.json").replace("\"locale\": \"en\"", &format!("\"locale\": \"en-{:03}\"", 100 + i)).replace("\"source\": true,", "");
            let mut texts: Vec<(String, Arc<str>)> = (0..n).map(|i| (format!("strings/x-{i:03}.json"), Arc::from(other(i)))).collect();
            texts.push(("strings/en.json".into(), Arc::from(builtin("assets/strings/en.json"))));
            texts
        };
        let counts = |shell: &Shell| shell.problems.iter().filter(|p| p.contains("assets/strings holds")).cloned().collect::<Vec<_>>();
        for n in [MAX_TABLES + 3, MAX_TABLES + 4, MAX_TABLES + 5] {
            shell.use_files(ShellFiles::of_texts(game.clone(), &tables(n)));
            assert_eq!(counts(&shell), [format!("assets/strings holds {} files: the first {MAX_TABLES} are read", n + 1)], "after {n} tables: {:?}", shell.problems);
        }
        assert!(shell.problems.iter().any(|p| p == "something else"), "{:?}", shell.problems);
        // Fewer tables than the limit: the count is no longer said; the other problem is.
        shell.use_files(ShellFiles::of_texts(game, &tables(0)));
        assert!(counts(&shell).is_empty(), "{:?}", shell.problems);
        assert!(shell.problems.iter().any(|p| p == "something else"));
    }

    /// The player's file holds the player's choices however the game file changes (review R2
    /// of M6 part 3: an agent rewrote the game file so that the player's choice no longer fit;
    /// the layer written back was rebuilt from the new file, and the player's next change of
    /// anything wrote the agent's new default as the player's own).
    #[test]
    fn a_changed_game_file_never_rewrites_the_players_choices() {
        let mut shell = shell();
        let store = SettingsStore { kind: "per_user", agents_save: false, problem: None };
        shell.use_settings(None, &store);
        shell.press("cycle:difficulty", true).unwrap();
        let text = shell.take_settings().expect("the settings file");
        assert!(text.contains("\"difficulty\": \"hard\"") && !text.contains("volume"), "only what the player chose: {text}");
        // An agent's game file: the difficulty takes other values now, with a default of its own.
        let changed = GAME.replace(r#""options": ["easy", "hard"]"#, r#""options": ["easy", "brutal"], "default": "brutal""#);
        assert_ne!(changed, GAME);
        shell.reload_game(&changed);
        assert_eq!(shell.settings.get("difficulty"), Some(&Val::Text("brutal".into())), "the session uses what the game takes now");
        // The player changes something else: their file still says what they chose.
        shell.press("step:volume:-1", true).unwrap();
        let text = shell.take_settings().expect("the settings file");
        assert!(text.contains("\"difficulty\": \"hard\"") && text.contains("\"volume\": 7"), "{text}");
        // The game file as it was: the player's choice is used again, with no one setting it.
        shell.reload_game(GAME);
        assert_eq!(shell.settings.get("difficulty"), Some(&Val::Text("hard".into())));
        assert_eq!(shell.settings.get("volume"), Some(&Val::Num(7.0)));
        // And a choice the player makes among the new values replaces the old one.
        shell.reload_game(&changed);
        shell.press("set:difficulty:easy", true).unwrap();
        assert!(shell.take_settings().expect("the settings file").contains("\"difficulty\": \"easy\""));
        // A started shell given a file whose choice does not fit uses the default and keeps the file's.
        let mut again = super::tests::shell();
        again.use_settings(Some("{\"format\": \"specula.settings.v1\", \"values\": {\"difficulty\": \"brutal\"}}"), &store);
        assert_eq!(again.settings.get("difficulty"), Some(&Val::Text("easy".into())));
        again.press("step:volume:-1", true).unwrap();
        assert!(again.take_settings().expect("the settings file").contains("\"difficulty\": \"brutal\""));
    }

    #[test]
    fn what_a_module_keeps_is_what_a_screen_is_given() {
        assert_eq!(Shown::of(&Value::Int(3)), Some(Shown::Int(3)));
        // A screenful of text is shown whole, its lines with it (a day's summary in one kept
        // text, 2026-11-23); past a label's own bound it is not, and why is said
        // (`a_kept_value_no_screen_is_given_is_said_with_why`).
        let summary: String = (1..=60).map(|i| format!("Line {i:02}: sold 12 apples for 36 coins, 3 spoiled\n")).collect();
        assert!(summary.len() > 2000 && summary.len() <= MAX_SHOWN_TEXT);
        assert_eq!(Shown::of(&Value::Str(summary.clone())), Some(Shown::Text(summary)));
        assert_eq!(Shown::of(&Value::Str("x".repeat(MAX_SHOWN_TEXT))), Some(Shown::Text("x".repeat(MAX_SHOWN_TEXT))));
        assert_eq!(Shown::of(&Value::Str("x".repeat(MAX_SHOWN_TEXT + 1))), None, "past a label's bound");
        assert_eq!(MAX_SHOWN_TEXT, specula_ui::state::MAX_TEXT, "what is given is what a label shows whole");
        assert_eq!(Shown::of(&Value::Null), None);
        let msg = Value::map().with("msg", "hud.wave").with("args", Value::map().with("wave", 2i64).with("of", 5i64));
        assert_eq!(Shown::of(&msg), Some(Shown::Msg("hud.wave".into(), vec![("of".into(), Shown::Int(5)), ("wave".into(), Shown::Int(2))])));
        let rows = Value::List(vec![Value::map().with("name", "ann").with("time", 12.5), Value::Int(1)]);
        assert_eq!(Shown::of(&rows), Some(Shown::Rows(vec![vec![("name".into(), Shown::Text("ann".into())), ("time".into(), Shown::Num(12.5))]])));
        // The comparison made in place says what making the shown value and comparing would,
        // for every pair of these (it is what keeps an unchanged value from being copied).
        let values = [
            Value::Bool(true),
            Value::Bool(false),
            Value::Int(3),
            Value::Int(4),
            Value::Num(3.0),
            Value::Num(f64::NAN),
            Value::Str("ann".into()),
            Value::Str("bo".into()),
            Value::Str("x".repeat(MAX_SHOWN_TEXT + 1)),
            Value::Null,
            msg.clone(),
            Value::map().with("msg", "hud.wave"),
            Value::map().with("msg", "hud.wave").with("args", Value::map().with("wave", 3i64).with("of", 5i64)),
            Value::map().with("msg", "hud.wave").with("args", Value::map().with("wave", 2i64).with("of", 5i64).with("skip", Value::Null)),
            Value::map().with("other", 1i64),
            rows.clone(),
            Value::List(vec![Value::map().with("name", "ann").with("time", 12.5)]),
            Value::List(vec![Value::map().with("name", "ann").with("time", 13.5)]),
            Value::List(vec![Value::map().with("name", "ann").with("time", 12.5), Value::map().with("name", "bo")]),
            Value::List(Vec::new()),
        ];
        for a in &values {
            let Some(shown) = Shown::of(a) else { continue };
            for b in &values {
                assert_eq!(shown.is(b), Shown::of(b).as_ref() == Some(&shown), "{a:?} shown, against {b:?}");
            }
        }
        // In the state a screen binds to, and as a condition reads it.
        let mut shell = shell();
        shell.shown = vec![("gate.hp".into(), Shown::Int(3)), ("gate.score".into(), Shown::Num(40.0)), ("gate.wave".into(), Shown::of(&msg).unwrap()), ("gate.order".into(), Shown::of(&rows).unwrap())];
        shell.fill();
        assert_eq!(shell.state.get("gate.hp").and_then(|v| v.as_f64()), Some(3.0));
        assert_eq!(shell.state.get("game.score").and_then(|v| v.as_f64()), Some(40.0));
        assert!(shell.state.get("game.has_score").is_some_and(|v| v.truthy()));
        assert_eq!(shell.state.list("gate.order").map(|l| l.len()), Some(1));
        assert_eq!(shell.state.get("setting.volume").and_then(|v| v.as_f64()), Some(8.0));
        assert!(shell.state.get("setting.language.text").is_some(), "the language's own name");
        assert_eq!((shell.value("gate.hp"), shell.value("setting.difficulty"), shell.value("gate.wave")), (Some(Val::Num(3.0)), Some(Val::Text("easy".into())), None));
        let state = shell.game_state().unwrap();
        assert_eq!(state.get("score").and_then(Json::as_f64), Some(40.0));
        assert_eq!(state.get("reads").map(Json::to_string).as_deref(), Some(r#"{"gate.hp":3,"gate.score":40,"gate.won":null}"#));
    }

    /// A day's summary of many lines in one kept text reaches the screen that binds it whole:
    /// in the state, in the label laid out, and in the tree a run reads its texts from.
    #[test]
    fn a_long_text_of_many_lines_is_shown_whole() {
        let mut files = ShellFiles::builtin(GAME);
        let screen = r#"{"format": "specula.ui.screen.v1", "name": "sum", "input": "hud", "root": {"type": "panel", "children": [{"type": "label", "text": "{work.disp}"}]}}"#;
        files.screens.push(("sum".into(), screen.into()));
        let mut shell = Shell::build("gate", files).expect("a shell");
        let summary: String = (1..=16).map(|i| format!("Stall {i:02}: 12 sold, 36 coins, 1 spoiled\n")).collect::<String>() + "Day 3 ends: 576 coins";
        assert!(summary.len() > 600);
        let store: BTreeMap<String, Value> = [("disp".to_owned(), Value::Str(summary.clone()))].into();
        shell.see_each([("work", &store)].into_iter());
        assert!(shell.unshown.is_empty(), "{:?}", shell.unshown);
        shell.refresh();
        assert!(matches!(shell.state.get("work.disp"), Some(specula_ui::state::Value::Str(s)) if *s == summary));
        let display = shell.display;
        let face = shell.face.as_mut().expect("an interface");
        face.ui.show("sum").unwrap();
        let view = Shell::viewport(&face.ui, &display, (800, 480), 1.0);
        let opts = specula_ui::agent::TreeOpts { screen: Some("sum".into()), text_chars: specula_ui::state::MAX_TEXT, ..Default::default() };
        let tree = face.ui.inspect(&shell.state, &view, |ui| ui.tree_page(&opts));
        let texts: Vec<&str> = tree.get("widgets").and_then(Json::as_array).unwrap().iter().filter_map(|w| w.get("text").and_then(Json::as_str)).collect();
        assert_eq!(texts, [summary.as_str()], "the label shows the text whole, its lines with it");
        // A tree asked for no more than its own entries cuts it, as it always did.
        let short = face.ui.inspect(&shell.state, &view, |ui| ui.tree_page(&specula_ui::agent::TreeOpts { screen: Some("sum".into()), ..Default::default() }));
        let text = short.get("widgets").and_then(Json::as_array).unwrap()[0].get("text").and_then(Json::as_str).unwrap().to_owned();
        assert_eq!(text.chars().count(), specula_ui::agent::MAX_ENTRY_TEXT + 1);
        assert!(text.ends_with('…'));
    }

    /// What the modules keep that no screen is given is listed with why, in `game_state`:
    /// a text past the bound with its length, a kind with what a screen can show; and taken
    /// off the list when it is shown again. The list is bounded.
    #[test]
    fn a_kept_value_no_screen_is_given_is_said_with_why() {
        let mut shell = shell();
        let mut store: BTreeMap<String, Value> = BTreeMap::new();
        store.insert("disp".into(), Value::Str("y".repeat(MAX_SHOWN_TEXT + 10)));
        store.insert("hp".into(), Value::Int(3));
        store.insert("inv".into(), Value::map().with("apples", 3i64));
        store.insert("none".into(), Value::Null);
        shell.see_each([("gate", &store)].into_iter());
        let state = shell.game_state().unwrap();
        let listed: Vec<(String, String)> = state.get("not_shown").and_then(Json::as_array).unwrap().iter().map(|n| (n.get("key").and_then(Json::as_str).unwrap().to_owned(), n.get("why").and_then(Json::as_str).unwrap().to_owned())).collect();
        assert_eq!(listed.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["gate.disp", "gate.inv", "gate.none"]);
        assert!(listed[0].1.contains(&format!("{} bytes", MAX_SHOWN_TEXT + 10)) && listed[0].1.contains(&format!("the {MAX_SHOWN_TEXT}")), "{}", listed[0].1);
        assert!(listed[1].1.contains("without a \"msg\"") && listed[2].1.starts_with("null"), "{listed:?}");
        assert_eq!(shell.value("gate.hp"), Some(Val::Num(3.0)), "what is shown is as it was");
        assert!(state.get("not_shown_more").is_none());
        // Shorter, it is shown, and no longer listed.
        store.insert("disp".into(), Value::Str("Day 1".into()));
        store.remove("inv");
        store.remove("none");
        shell.see_each([("gate", &store)].into_iter());
        assert!(shell.game_state().unwrap().get("not_shown").is_none());
        assert_eq!(shell.value("gate.disp"), Some(Val::Text("Day 1".into())));
        // Bounded: the first so many, and that there were more.
        let many: BTreeMap<String, Value> = (0..MAX_UNSHOWN + 5).map(|i| (format!("k{i:03}"), Value::Null)).collect();
        shell.see_each([("gate", &many)].into_iter());
        let state = shell.game_state().unwrap();
        assert_eq!(state.get("not_shown").and_then(Json::as_array).map(<[Json]>::len), Some(MAX_UNSHOWN));
        assert_eq!(state.get("not_shown_more").and_then(Json::as_bool), Some(true));
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
        shell.apply(effects, false);
        assert_eq!(shell.take_clock(), Some(true), "the clock stops with the run");
        assert_eq!(events(&mut shell), [("outcome".to_owned(), Value::map().with("outcome", "lost").with("tick", 77i64))]);
        let state = shell.game_state().unwrap();
        assert_eq!((state.get("outcome").and_then(Json::as_str), state.get("ended_at").and_then(Json::as_u64)), (Some("lost"), Some(77)));
        shell.sync_screens();
        assert_eq!(shell.face.as_ref().unwrap().ui.visible_screens().collect::<Vec<_>>(), ["hud", "lost"]);
        // The game's part of a save, and back.
        let bytes = shell.save_state().unwrap().unwrap();
        let parsed = shell.parse_state(&bytes).unwrap();
        let saved = parsed.downcast::<(Phase, Option<u64>, u64, Vec<(String, Value)>)>().unwrap();
        assert_eq!(*saved, (Phase::Ended(Ended::Lost), Some(77), 1, Vec::new()));
        // What the modules have still to be told is in the save, each number the kind it
        // was, and a load tells it in place of whatever waited before the load.
        shell.events_out = vec![("run".to_owned(), Value::map().with("phase", "start").with("tick", 5i64).with("speed", 0.5))];
        let bytes = shell.save_state().unwrap().unwrap();
        shell.events_out = vec![("ui".to_owned(), Value::map().with("action", "pressed_before_the_load"))];
        let parsed = shell.parse_state(&bytes).unwrap();
        shell.put_back(parsed);
        assert_eq!(events(&mut shell), [("run".to_owned(), Value::map().with("phase", "start").with("tick", 5i64).with("speed", 0.5))]);
        assert!(shell.parse_state(br#"{"format": "specula.shell.save.v1", "phase": "running", "events": [["run", "not base64 of a value"]]}"#).is_err());
        assert!(shell.parse_state(b"{}").is_err() && shell.parse_state(br#"{"format": "specula.shell.save.v1", "phase": "flying"}"#).is_err());
    }

    /// A button's word that goes to the game says which button, on which screen.
    #[test]
    fn a_games_own_word_carries_its_button_and_screen() {
        let mut shell = shell();
        shell.press("start", true).unwrap();
        events(&mut shell);
        shell.press_from("build_wall", Some(("wall_button", "hud")), true).unwrap();
        assert_eq!(events(&mut shell), [("ui".to_owned(), Value::map().with("action", "build_wall").with("widget", "wall_button").with("screen", "hud"))]);
        // The shell's own words are not the game's: nothing reaches the modules.
        shell.press_from("open:settings", Some(("settings", "pause")), true).unwrap();
        assert_eq!(events(&mut shell), []);
        // What a screen may show of the run itself.
        shell.fill();
        let text = |key: &str| shell.state.get(key).map(|v| format!("{v:?}"));
        assert!(text("game.screen").is_some_and(|s| s.contains("settings")), "{:?}", text("game.screen"));
        assert!(text("game.outcome").is_some_and(|s| s.contains("running")));
        assert!(text("setting.difficulty.text").is_some_and(|s| s.contains("easy")));
        assert!(text("setting.volume.text").is_some_and(|s| s.contains('8')));
    }

    /// A script's ticks are the game's and no others (`specula run`): the clock itself stands
    /// still, a tick runs only while the game has its run's clock running, and the game is
    /// never told that someone else stopped it. (The measured comparison's builders stepped a
    /// clock the game had started, and got ticks of the machine's own time: one game failed
    /// a check on it, and two were given a hidden action to freeze them.)
    #[test]
    fn a_scripts_ticks_run_only_while_the_game_has_its_clock_running() {
        let Some(mut e) = crate::workers::charge_tests::engine() else { return };
        let phase = |out: &specula_protocol::ToolOutput| match &out.content[0] {
            specula_protocol::Content::Text(t) => Json::parse(t).unwrap().get("phase").and_then(Json::as_str).map(str::to_owned),
            specula_protocol::Content::Image { .. } => None,
        };
        let who = specula_protocol::SessionInfo { id: "script-test".into(), client_name: "t".into(), client_version: "1".into(), protocol_version: "x".into() };
        e.game_name = "gate".into();
        e.game = Some(Box::new(shell()));
        // As `--script` starts an engine: the clock is the script's before the game's first wish.
        e.clock.set_paused(true);
        e.script_clock = Some(false);
        e.start_game();
        // The title holds the game's clock: no tick, however many are asked.
        assert_eq!((e.play_ticks(10), e.clock.tick()), (0, 0));
        assert_eq!(phase(&e.game_act(&who, "start")).as_deref(), Some("running"));
        assert!(e.clock.paused(), "the clock itself stands still: no time of this machine becomes a tick");
        assert_eq!(e.script_clock, Some(true), "the game has its run's clock running");
        assert_eq!((e.play_ticks(10), e.clock.tick()), (10, 10));
        // The game is not told that someone stopped its clock: its run is still under way.
        e.game_sees();
        assert_eq!(phase(&e.game_state()).as_deref(), Some("running"));
        // A pause holds it, a resume gives it back, and the title holds it again.
        assert_eq!(phase(&e.game_act(&who, "pause")).as_deref(), Some("paused"));
        assert_eq!((e.play_ticks(10), e.clock.tick()), (0, 10));
        assert_eq!(phase(&e.game_act(&who, "resume")).as_deref(), Some("running"));
        assert_eq!((e.play_ticks(5), e.clock.tick()), (5, 15));
        e.game_act(&who, "title");
        assert_eq!((e.play_ticks(5), e.clock.tick()), (0, 15));
        // The operator's own session is a caller inside the engine, not a connection: it counts
        // as open, so that what it owns (a frame being encoded) is not dropped as a closed
        // session's.
        assert!(!e.open_session_ids(Vec::new()).contains(crate::run_modules::RUN_SESSION));
        e.swarm.admit_run_operator(crate::run_modules::RUN_SESSION);
        assert!(e.open_session_ids(Vec::new()).contains(crate::run_modules::RUN_SESSION));
        // More than one advance runs is asked for again by the script, never queued behind it.
        e.game_act(&who, "start");
        let ran = e.play_ticks(100_000);
        assert!(ran <= crate::game::STEPS_AT_MOST && e.clock.queued() == 0, "{ran}");
    }

    /// Quit under a script (`specula run`): the script stands in for the player, so its press
    /// of Quit from a frame, or the rules' own `quit`, asks to quit. An agent's press by word
    /// (`game_act`) is refused there too, and without a script neither is anyone's but the
    /// window's player's, as it was.
    #[test]
    fn under_a_script_its_quit_and_the_rules_quit_are_the_players() {
        let game = GAME.replace("\"outcome\"", "\"quit\": \"gate.quit\", \"outcome\"");
        let mut windowed = Shell::build("gate", ShellFiles::builtin(&game)).unwrap();
        // Not a script's: a synthetic frame's Quit is an agent's, the window's own the player's.
        assert!(!windowed.as_player("quit", false) && windowed.as_player("quit", true));
        assert!(windowed.press_from("quit", None, windowed.as_player("quit", false)).is_err());
        assert!(!windowed.take_quit());
        windowed.press_from("quit", None, windowed.as_player("quit", true)).unwrap();
        assert!(windowed.take_quit());
        // The rules ask to quit: said once, and nothing quits.
        windowed.press("start", true).unwrap();
        windowed.shown = vec![("gate.quit".into(), Shown::Bool(true))];
        windowed.rules_quit();
        windowed.rules_quit();
        assert!(!windowed.take_quit());
        assert_eq!(windowed.problems.iter().filter(|p| p.contains("the rules asked to quit")).count(), 1, "{:?}", windowed.problems);
        // A script's: its frame's Quit is the player's, and nothing else of the player's alone is.
        let mut scripted = Shell::build("gate", ShellFiles::builtin(&game)).unwrap();
        scripted.played_by_script();
        assert!(scripted.as_player("quit", false) && !scripted.as_player("start", false));
        scripted.press_from("quit", None, scripted.as_player("quit", false)).unwrap();
        assert!(scripted.take_quit());
        // An agent's press by word (what `game_act` does) is refused under a script as anywhere.
        assert_eq!(scripted.press_from("quit", None, false).map_err(|(code, _)| code), Err("players_only"));
        assert!(!scripted.take_quit());
        // The rules' quit, while its key is true (a number not 0 too), and not while it is not.
        scripted.press("start", true).unwrap();
        for (value, quits) in [(Shown::Bool(false), false), (Shown::Int(0), false), (Shown::Bool(true), true), (Shown::Int(2), true)] {
            scripted.shown = vec![("gate.quit".into(), value)];
            scripted.rules_quit();
            assert_eq!(scripted.take_quit(), quits);
        }
        assert!(scripted.problems.iter().all(|p| !p.contains("the rules asked to quit")));
        // A quit asked before a save is loaded was asked in another run: the game put back
        // has asked for none (`specula find` loads a save after a way that quit).
        scripted.shown = vec![("gate.quit".into(), Shown::Bool(true))];
        scripted.rules_quit();
        let saved = scripted.save_state().unwrap().unwrap();
        let parsed = scripted.parse_state(&saved).unwrap();
        scripted.put_back(parsed);
        assert!(!scripted.take_quit(), "a load takes the waiting quit away");
        // A game file with no `quit` asks for nothing, whatever its rules keep.
        let mut plain = shell();
        plain.played_by_script();
        plain.shown = vec![("gate.quit".into(), Shown::Bool(true))];
        plain.rules_quit();
        assert!(!plain.take_quit());
    }

    /// A game that asks to quit under a script has no further tick: none after the one it asked
    /// on, none in the script's next ticks until the script was told (once), and the engine of
    /// no script takes its quit at all (a window's is the window loop's).
    #[test]
    fn under_a_script_a_quit_ends_the_ticks_where_the_game_stood() {
        let Some(mut e) = crate::workers::charge_tests::engine() else { return };
        /// A game whose rules ask to quit at its third tick; with `hold`, its clock stops on that
        /// same tick (its run ended there, or it paused).
        struct QuitsAt {
            ticks: u64,
            quit: bool,
            clock: Option<bool>,
            hold: bool,
        }
        impl Game for QuitsAt {
            fn name(&self) -> &str {
                "quits"
            }
            fn tick(&mut self, _ctx: &mut Ctx) {
                self.ticks += 1;
                self.quit |= self.ticks == 3;
                if self.ticks == 3 && self.hold {
                    self.clock = Some(true);
                }
            }
            fn take_quit(&mut self) -> bool {
                std::mem::take(&mut self.quit)
            }
            fn take_clock(&mut self) -> Option<bool> {
                self.clock.take()
            }
        }
        e.game_name = "quits".into();
        e.game = Some(Box::new(QuitsAt { ticks: 0, quit: false, clock: Some(false), hold: false }));
        e.clock.set_paused(true);
        e.script_clock = Some(false);
        e.start_game();
        assert_eq!(e.play_ticks(10), 3, "the tick it asked on is the last");
        assert_eq!((e.play_ticks(10), e.clock.tick()), (0, 3), "and no tick runs until the script was told");
        assert!(std::mem::take(&mut e.script_quit), "told once");
        assert_eq!(e.play_ticks(2), 2, "a game put back (a search's load) goes on");
        // The clock held on the very tick the game quits (it won, or paused, and quit): the quit
        // is taken on that tick, with the ticks that tell the script so.
        let before = e.clock.tick();
        e.script_quit = false;
        e.script_clock = Some(false);
        e.game = Some(Box::new(QuitsAt { ticks: 0, quit: false, clock: Some(false), hold: true }));
        assert_eq!(e.play_ticks(10), 3);
        assert!(e.script_quit, "the quit is taken on the tick its clock stopped, not at the next call");
        assert_eq!((e.clock.tick() - before, e.script_clock), (3, Some(false)), "and the clock is held as the game asked");
        e.script_quit = false;
        // No script: the game's quit is not taken here.
        e.script_clock = None;
        e.game = Some(Box::new(QuitsAt { ticks: 0, quit: true, clock: None, hold: false }));
        assert!(!e.script_quit_asked());
        assert_eq!(e.game.as_mut().map(|g| g.take_quit()), Some(true), "left for the window loop, which a headless engine has not");
    }

    /// Whether the window's presses act in the game is the game's own clock wish (its title, its
    /// pause, an ended run hold it), never the engine clock: an agent's pause, a step or a
    /// playtest stepping the clock leaves a person's presses latched as ever.
    #[test]
    fn the_game_holds_the_clock_by_its_own_wish_and_an_agents_pause_is_not_it() {
        let Some(mut e) = crate::workers::charge_tests::engine() else { return };
        let who = specula_protocol::SessionInfo { id: "held-test".into(), client_name: "t".into(), client_version: "1".into(), protocol_version: "x".into() };
        e.game_name = "gate".into();
        e.game = Some(Box::new(shell()));
        e.start_game();
        e.serve_game_clock();
        assert!(e.game_holds_clock, "the title holds it");
        e.game_act(&who, "start");
        assert!(!e.game_holds_clock && !e.clock.paused());
        // An agent pauses the engine's clock, or steps it: the game's own run is under way.
        e.clock.set_paused(true);
        e.serve_game_clock();
        assert!(!e.game_holds_clock, "an agent's pause is not the game's");
        e.playtest_pumping = true;
        e.game_act(&who, "pause");
        assert!(e.game_holds_clock, "the game's own pause, even while a playtest has the clock");
        e.playtest_pumping = false;
        e.game_act(&who, "resume");
        assert!(!e.game_holds_clock);
    }

    /// The engine does what a press asks of the clock, except while a playtest has the clock:
    /// then the playtest's own steps tick the run, and the clock stays as the playtest holds it
    /// (found by the first live run of a new project's own check: the game's start un-paused
    /// the clock the playtest was stepping, and the playtest ended with "the clock didn't
    /// advance").
    #[test]
    fn a_press_moves_the_clock_unless_a_playtest_has_it() {
        let Some(mut e) = crate::workers::charge_tests::engine() else { return };
        let text = |out: &specula_protocol::ToolOutput| match &out.content[0] {
            specula_protocol::Content::Text(t) => Json::parse(t).unwrap(),
            specula_protocol::Content::Image { .. } => Json::Null,
        };
        let who = specula_protocol::SessionInfo { id: "shell-test".into(), client_name: "t".into(), client_version: "1".into(), protocol_version: "x".into() };
        e.game_name = "gate".into();
        e.game = Some(Box::new(shell()));
        e.clock.set_paused(true);
        // No game's word reaches a game that takes none; ours takes them.
        let started = e.game_act(&who, "start");
        assert!(!started.is_error, "{:?}", text(&started));
        assert_eq!(text(&started).get("phase").and_then(Json::as_str), Some("running"));
        assert!(!e.clock.paused(), "the run began: the clock runs");
        assert_eq!(e.clock_set_by.as_ref().map(|(by, op)| (by.as_str(), *op)), Some(("game:gate", "resume")), "time_control says the game did it");
        let paused = e.game_act(&who, "pause");
        assert_eq!((text(&paused).get("phase").and_then(Json::as_str), e.clock.paused()), (Some("paused"), true));
        let refused = e.game_act(&who, "quit");
        assert!(refused.is_error && text(&refused).get("error").and_then(Json::as_str) == Some("players_only"));
        // (Taken by the engine with the call and told to the input monitor under its caller:
        // none is left for whoever calls next.)
        assert_eq!(e.game.as_mut().map(|g| g.take_refused_presses()), Some(0));
        // A playtest holds the clock paused and steps it: the game's wish changes nothing.
        e.game_act(&who, "title");
        e.playtest_pumping = true;
        assert!(e.playtest_owns_clock());
        let started = e.game_act(&who, "start");
        assert_eq!(text(&started).get("phase").and_then(Json::as_str), Some("running"));
        assert!(e.clock.paused(), "the playtest's clock is left as it holds it");
        e.playtest_pumping = false;
        // The wish is kept, the last one, and done when the playtest gives the clock back (it
        // took a stopped clock, at the title): the run that began in it is under way, so its
        // clock runs (review R1 of M6 part 3: thrown away, it left a run under way with a
        // stopped clock that no word could start).
        e.playtest_clock_back(true);
        assert!(!e.clock.paused(), "the run began in the playtest: its clock runs after it");
        // A run that ended in a playtest (which took a running clock) has its clock stopped
        // after it: the game's wish, not the clock as the playtest found it.
        e.clock.set_paused(true);
        e.playtest_pumping = true;
        e.game_act(&who, "title");
        e.playtest_pumping = false;
        e.playtest_clock_back(false);
        assert!(e.clock.paused(), "given up in the playtest: the title holds the clock after it");
        // A wish is done once: a playtest in which the game wished nothing gives the clock
        // back as it found it.
        e.playtest_clock_back(true);
        assert!(e.clock.paused());
        e.clock.set_paused(true);
        e.playtest_clock_back(false);
        assert!(!e.clock.paused(), "nothing was wished: the clock is as the playtest found it");
        e.clock.set_paused(true);
        // With no game hosted the tools say so.
        e.game = None;
        assert_eq!(text(&e.game_act(&who, "start")).get("error").and_then(Json::as_str), Some("no_game"));
        assert_eq!(text(&e.game_state()).get("error").and_then(Json::as_str), Some("no_game"));
    }

    /// Every screen the shell can open over another is drawn above it: the built-in screens'
    /// layers are in the order they stack, and a project's screen that is not is named.
    #[test]
    fn a_screen_opened_over_another_is_above_it_or_is_said() {
        let mut shell = shell();
        // Each place the run can stand, with the settings opened over it.
        let stand = |shell: &mut Shell| {
            shell.fill();
            shell.sync_screens();
        };
        stand(&mut shell);
        shell.press("open:settings", true).unwrap();
        stand(&mut shell);
        shell.press("back", true).unwrap();
        shell.press("start", true).unwrap();
        stand(&mut shell);
        shell.press("pause", true).unwrap();
        stand(&mut shell);
        shell.press("open:settings", true).unwrap();
        stand(&mut shell);
        assert_eq!(shell.run.visible(), ["hud", "pause", "settings"]);
        for outcome in [Ended::Won, Ended::Lost] {
            shell.run.restore(Phase::Ended(outcome), Some(1), 1);
            stand(&mut shell);
        }
        assert_eq!(shell.problems, Vec::<String>::new());
        // A project's own screen with a layer below the one it opens over.
        let mut files = ShellFiles::builtin(GAME);
        files.screens.push(("shop".into(), builtin("assets/ui/pause.json").replace("\"name\": \"pause\"", "\"name\": \"shop\"").replace("\"layer\": 20", "\"layer\": 5")));
        let mut shell = Shell::build("gate", files).unwrap();
        shell.press("open:shop", true).unwrap();
        stand(&mut shell);
        assert_eq!(shell.problems.len(), 1, "{:?}", shell.problems);
        assert!(shell.problems[0].contains("shop.json opens over title") && shell.problems[0].contains("higher layer"), "{:?}", shell.problems);
    }

    /// `game_act`: an agent's press by word answers with a code to branch on when it is refused.
    #[test]
    fn an_agents_press_by_word_is_done_or_refused_with_a_code() {
        let mut shell = shell();
        let code = |shell: &mut Shell, word: &str| shell.press_from(word, None, false).map_err(|(code, _)| code);
        assert_eq!(code(&mut shell, "pause"), Err("not_now"), "nothing to pause at the title");
        assert_eq!(code(&mut shell, "start"), Ok(()));
        assert_eq!(shell.run.phase(), Phase::Running);
        assert_eq!(code(&mut shell, "start"), Err("not_now"));
        assert_eq!(code(&mut shell, "quit"), Err("players_only"));
        assert_eq!(code(&mut shell, "open:"), Err("bad_action"));
        assert_eq!(code(&mut shell, "open:shop"), Err("no_screen"));
        assert_eq!(code(&mut shell, "set:difficulty:impossible"), Err("bad_setting"));
        assert_eq!(code(&mut shell, "set:difficulty:hard"), Ok(()));
        assert_eq!(code(&mut shell, "pause"), Ok(()));
        assert_eq!((shell.run.phase(), shell.take_clock()), (Phase::Paused, Some(true)));
        // The setting the rules read reached the modules, with the run's start before it.
        let heard: Vec<String> = events(&mut shell).into_iter().map(|(name, _)| name).collect();
        assert_eq!(heard, ["setting", "run", "setting"]);
    }

    /// The interface's scale and the text's size are settings a game may declare, in per cent.
    #[test]
    fn scale_and_text_size_are_settings_in_per_cent() {
        let game = GAME.replace(
            r#""settings": ["#,
            r#""settings": [{ "name": "ui", "kind": "int", "min": 75, "max": 200, "default": 100, "is": "ui_scale" }, { "name": "text", "kind": "int", "min": 50, "max": 300, "default": 100, "is": "text_size" }, "#,
        );
        let mut shell = Shell::build("gate", ShellFiles::builtin(&game)).unwrap();
        assert_eq!((shell.scale_of("ui_scale"), shell.scale_of("text_size")), (1.0, 1.0));
        let as_designed = shell.face.as_ref().unwrap().ui.theme().text_scale;
        shell.press("set:ui:150", true).unwrap();
        shell.press("set:text:200", true).unwrap();
        assert_eq!((shell.scale_of("ui_scale"), shell.scale_of("text_size")), (1.5, 2.0));
        assert_eq!(shell.ui(UiPick::Live).map(|u| u.user_scale), Some(1.5));
        assert_eq!(shell.face.as_ref().unwrap().ui.theme().text_scale, as_designed * 2.0, "the theme is the project's, at the player's size");
        assert_eq!(shell.problems, Vec::<String>::new());
        // A game that declares neither is as designed.
        assert_eq!(super::tests::shell().scale_of("ui_scale"), 1.0);
    }

    /// The game file is followed while the game runs: a new outcome and new settings are the
    /// game's at once, a file that is not a game file changes nothing, and where the run stands
    /// and what the player chose are kept.
    #[test]
    fn a_changed_game_file_is_followed_and_a_broken_one_is_not() {
        let mut shell = shell();
        shell.press("start", true).unwrap();
        shell.press("cycle:difficulty", true).unwrap();
        shell.press("step:volume:-1", true).unwrap();
        events(&mut shell);
        shell.reload_game("{ not a game file");
        assert_eq!(shell.problems.len(), 1, "{:?}", shell.problems);
        assert!(shell.problems[0].starts_with("game.json:") && shell.problems[0].contains("as it was"), "{:?}", shell.problems);
        assert_eq!(shell.game.title, "Hold the gate");
        // A new outcome, a new setting, another title, another tick rate.
        let next = GAME
            .replace("Hold the gate", "Hold the bridge")
            .replace(r#""lost": { "key": "gate.hp", "lte": 0 }"#, r#""lost": { "key": "gate.hp", "lte": 2 }"#)
            .replace(r#""sim": true }"#, r#""sim": true }, { "name": "hints", "kind": "toggle", "default": true }"#)
            .replace(r#""tick_hz": 60"#, r#""tick_hz": 30"#);
        shell.reload_game(&next);
        assert_eq!((shell.game.title.as_str(), shell.run.phase(), shell.run.runs()), ("Hold the bridge", Phase::Running, 1));
        assert_eq!((shell.settings.get("difficulty"), shell.settings.get("volume"), shell.settings.get("hints")), (Some(&Val::Text("hard".into())), Some(&Val::Num(7.0)), Some(&Val::Bool(true))));
        assert_eq!(shell.tick_hz(), 60, "this run's clock keeps its rate");
        assert!(shell.problems.iter().any(|p| p.contains("tick_hz 30 is used from the next start")), "{:?}", shell.problems);
        // The new condition ends the run where the old one would not have.
        shell.shown = vec![("gate.hp".into(), Shown::Int(2))];
        let mut run = shell.run.clone();
        let effects = run.after_tick(&|key: &str| shell.value(key), 9);
        shell.run = run;
        shell.apply(effects, false);
        assert_eq!(shell.run.phase(), Phase::Ended(Ended::Lost));
        // The same text again changes nothing (no second note).
        let before = shell.problems.len();
        shell.reload_game(&next);
        assert_eq!(shell.problems.len(), before);
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
        assert!(shell.state.get("setting.language").is_some() && shell.state.get("setting.language.text").is_some());
        // The project's own screen opens by its name.
        shell.press("open:shop", true).unwrap();
        shell.sync_screens();
        assert!(shell.face.as_ref().unwrap().ui.is_visible("shop"));
        let _ = std::fs::remove_dir_all(dir);
    }
}

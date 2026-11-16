//! Games running inside the engine, and the fixed-tick clock they run on.
//!
//! A game is linked into its own binary for now (`specula::run_game`). The M3 module host will
//! load the same interface from a hot-reloadable library, so keep it narrow: the game owns its
//! simulation state, reads and writes the shared [`World`] through its public API (so agents see
//! and can edit everything the game builds), and adds its own MCP tools.
//!
//! Time (foundation F2): the simulation only knows the tick number. The clock turns real time into
//! whole ticks; rendering happens between ticks and gets `alpha` to interpolate.

use crate::sight::Camera;
use crate::world::World;
use specula_core::Json;
use specula_gpu::{OverlayQuad, OverlayTextures};
use specula_platform::{Event, GamepadState, MAX_GAMEPADS};
use specula_protocol::{ToolDef, ToolOutput};
use std::any::Any;
use std::path::PathBuf;
use std::time::Duration;

/// Most overlay quads a game may submit for one image ([`Game::overlay`]); the rest are dropped
/// (and counted in `perf_frame`). Leaves room under the GPU's limit for the debug overlays an agent's
/// capture adds on top.
pub const MAX_GAME_OVERLAY_QUADS: usize = 16_384;

/// A game hosted by the engine. Every method runs on the engine thread.
pub trait Game {
    /// Shown in `engine_info`; also the agent name on every change the game makes to the world.
    fn name(&self) -> &str;

    /// Simulation ticks per second (fixed for the whole run).
    fn tick_hz(&self) -> u32 {
        30
    }

    /// The command-line arguments after `--` (or `--game-args`), before anything starts: e.g.
    /// `sepulcra -- --seed 7`. An error stops the engine with that message. The default accepts
    /// none.
    fn configure(&mut self, args: &[String]) -> Result<(), String> {
        match args.first() {
            None => Ok(()),
            Some(a) => Err(format!("takes no arguments (got '{a}')")),
        }
    }

    /// Where this game's assets (model recipes, anim sets, LUTs) live when the command line has no
    /// `--assets`: e.g. the folder shipped next to the executable. `None`: the working directory.
    fn assets_root(&self) -> Option<PathBuf> {
        None
    }

    /// The assets root the engine settled on (`--assets`, else [`Game::assets_root`], else the
    /// working directory), once, before [`Game::start`]: the game reads its own files (screens,
    /// sounds) from here too, so one flag points the engine and the game at the same folder.
    fn use_assets(&mut self, _root: &std::path::Path) {}

    /// The engine's audio service, once, before [`Game::start`]: play sounds and music through
    /// it (naming the game as who played them) instead of opening a device of your own, so
    /// agents hear what the game plays (`audio_played`, `audio_levels`). It may be off (headless
    /// engines default to `--audio off`): check [`crate::audio_service::AudioService::enabled`].
    /// Never let audio feed back into the simulation.
    fn use_audio(&mut self, _audio: std::sync::Arc<crate::audio_service::AudioService>) {}

    /// Once, after the engine, window and world are ready (a loaded world may already hold
    /// the scene).
    fn start(&mut self, _ctx: &mut Ctx) {}

    /// One fixed simulation step. Not called while the clock is paused.
    fn tick(&mut self, _ctx: &mut Ctx) {}

    /// Once per window frame, after the ticks it caused and before rendering: read input, move
    /// the camera, update presentation. Headless engines never call it.
    fn frame(&mut self, _ctx: &mut Ctx, _frame: &mut Frame) {}

    /// The game's interface for one image ([`Overlay`]): quads drawn over the finished frame
    /// (after post-processing, never in the ID buffer). Called for every window frame right after
    /// [`Game::frame`], and for captures that ask for `include_ui` (`overlay.capture`: lay out at
    /// the capture's size and read no input). Keep it cheap and bounded: at most
    /// [`MAX_GAME_OVERLAY_QUADS`] quads are drawn.
    fn overlay(&mut self, _ctx: &mut Ctx, _overlay: &mut Overlay) {}

    /// Right before `sight_frame` renders a capture (headless too): bring presentation up to the
    /// current tick (a headless engine never calls [`Game::frame`], and a paused window may not
    /// have drawn since the last step). Changes are journaled like a frame's.
    fn before_capture(&mut self, _ctx: &mut Ctx) {}

    /// Lex native hooks the game implements in-process (e.g. `sepulcra.ground`): `lex_simulate`
    /// and `lex_live` runs call them like Lex's own building blocks, and validation doesn't warn
    /// about them. Default: none.
    fn lex_natives(&self) -> specula_lex::Natives {
        specula_lex::Natives::new()
    }

    /// The game's tools (of [`Game::tools`]) that write files, such as recordings: a playtest
    /// counts their calls toward its cap on file writes. Default: none.
    fn file_tools(&self) -> &[&str] {
        &[]
    }

    /// Extra MCP tools (by convention `play_*`), answered by [`Game::call_tool`].
    fn tools(&self) -> Vec<ToolDef> {
        Vec::new()
    }

    fn call_tool(&mut self, _ctx: &mut Ctx, name: &str, _args: &Json) -> ToolOutput {
        ToolOutput::error(format!("unknown game tool '{name}'"))
    }

    /// A short status object for `engine_info` (wave, score…). Keep it small: every agent that
    /// asks for engine info pays for it.
    fn status(&self) -> Json {
        Json::Null
    }

    /// The game's own simulation state as hashes by subsystem (`("sim", h)`, `("economy", h)`…),
    /// pushed into `out` as every tick begins, for the desync detector: `desync_compare` names
    /// the subsystems that differ between two runs. State a game keeps outside the world is
    /// invisible to the world's hash, so report it here. Runs every tick: keep it cheap and
    /// deterministic (same state, same hashes, on every machine). Default: none.
    fn state_hashes(&self, _out: &mut Vec<(&'static str, u64)>) {}

    /// A copy of everything the game keeps (simulation, presentation, cached entity ids), taken
    /// before an agent's game tool runs while agents hold claims. If a claim rejects the tool's
    /// world changes, the engine rolls the world back and hands this to [`Game::restore`], so game
    /// and world stay consistent. `None` (the default): no checkpoints; the engine calls
    /// [`Game::world_rolled_back`] instead.
    fn checkpoint(&self) -> Option<Box<dyn Any>> {
        None
    }

    /// Goes back to a [`Game::checkpoint`] (the world has already been rolled back to match).
    fn restore(&mut self, _checkpoint: Box<dyn Any>) {}

    /// Save games (docs/design/SAVES.md): everything the game keeps outside the world that its
    /// future depends on (its simulation, the run's recording, its own pause and speed), as
    /// bytes in the game's own versioned format. Presentation it can rebuild from that and the
    /// world stays out. `None` (the default): the game keeps nothing a save needs, or doesn't
    /// support saves (`save_write` then saves the world without it, and says so). `Some(Err)`
    /// refuses the save with that message.
    fn save_state(&self) -> Option<Result<Vec<u8>, String>> {
        None
    }

    /// Reads [`Game::save_state`]'s bytes without changing anything. An error refuses the load
    /// before the world is replaced, so it must catch everything [`Game::apply_state`] could
    /// trip on (format, version, content that changed since the save).
    fn parse_state(&self, _data: &[u8]) -> Result<Box<dyn Any>, String> {
        Err(format!("the game '{}' doesn't load save games", self.name()))
    }

    /// [`Game::parse_state`] as a function that can run on another thread: a load reads and checks
    /// a save on a worker, and only the swap (with [`Game::apply_state`]) runs on the engine
    /// thread. It must check what `parse_state` checks, against what the game is now (capture
    /// its content). `None` (the default): `parse_state` runs on the engine thread at the swap,
    /// so a game with big state should provide one.
    fn state_parser(&self) -> Option<StateParser> {
        None
    }

    /// Takes a [`Game::parse_state`] result, after the engine made the save's world live (and
    /// set the clock to the save's tick). The world already holds what the game had built:
    /// adopt those entities (look them up), don't rebuild them, or the world's hash moves and
    /// the loaded run no longer matches the one that saved. Transient presentation may be
    /// rebuilt freely.
    fn apply_state(&mut self, _ctx: &mut Ctx, _state: Box<dyn Any>) {}

    /// Requests from the game's own interface that need the engine (a menu's Save and Load),
    /// taken after every [`Game::frame`]. Default: none.
    fn take_requests(&mut self, _out: &mut Vec<GameRequest>) {}

    /// How a [`Game::take_requests`] request went: the tool's answer, or its error (both as
    /// `save_write` / `save_read` / `save_list` would give them). After a load the game's state
    /// is already the save's ([`Game::apply_state`] ran).
    fn request_done(&mut self, _ctx: &mut Ctx, _request: &GameRequest, _result: Result<Json, Json>) {}

    /// Project files under the assets root changed (written through the project tools, undone,
    /// or edited on disk), named relative to the assets root (`ui/hud.json`): reload what the
    /// game reads from them (screens, sounds). The disk already holds the new text. Model
    /// recipes, styles and anim sets are reloaded by the engine. Default: nothing.
    fn assets_changed(&mut self, _ctx: &mut Ctx, _paths: &[String]) {}

    /// The project's game file (`game.json`) changed: its new text, from the project store
    /// (written through the project tools, undone, or edited on disk). The project shell
    /// follows it while it runs. Default: nothing.
    fn game_file_changed(&mut self, _ctx: &mut Ctx, _text: &str) {}

    /// The project's interface files changed: the texts of every screen, the theme and every
    /// string table under the assets root (`ui/hud.json`, `strings/en.json`), as the project
    /// store holds them now. A game that takes its interface from these reads them here and
    /// nothing from the disk (this runs on the engine thread). Default: nothing.
    fn asset_texts(&mut self, _ctx: &mut Ctx, _texts: &[(String, std::sync::Arc<str>)]) {}

    /// An engine screen that takes the window's input (the review or the gallery screen) opened
    /// (`true`) or closed: a real-time game pauses itself through its own pause path, so the
    /// human doesn't lose while deciding (the call is journaled like any frame's). Default: nothing.
    fn modal_changed(&mut self, _ctx: &mut Ctx, _open: bool) {}

    /// The engine undid the world changes of a rejected game tool and the game had no checkpoint:
    /// make the world show the game's own state again. The entities those changes created are
    /// gone and their ids are **never reused**, so the game must drop every id it cached from the
    /// tool call (look entities up again rather than trusting stale ids).
    fn world_rolled_back(&mut self, _ctx: &mut Ctx) {}

    /// The display the game's interface lays out for: the window's (its size, the OS's scale
    /// for its monitor, safe-area insets) or, headless, the agents' virtual display (`ui_*`
    /// tools' `display`). Called when it changes, before the frame that uses it. Lay the live
    /// interface out with `display.viewport(theme, user_scale)` and a capture's with the same
    /// display at the capture's size, so agents measure what the player sees. Default: nothing.
    fn display(&mut self, _display: &specula_ui::Display) {}

    /// Its interface's language and the string-table keys its code uses (`l10n_get`,
    /// `l10n_check`; docs/design/LOCALISATION.md). A cheap read: no disk, no layout.
    fn locale(&self) -> GameLocale {
        GameLocale::default()
    }

    /// The game's interface for the engine's `ui_tree`, `ui_frame`, `ui_act` and `ui_check`
    /// ([`UiPick`]). [`UiPick::Live`] first brings the state the screens bind to up to date (as a
    /// frame would). `None`: the game has no `specula-ui` interface. Default: `None`.
    fn ui(&mut self, _pick: UiPick) -> Option<GameUi<'_>> {
        None
    }

    /// Timelines the game wants played or stopped (an intro, a victory shot), taken after every
    /// tick and frame (docs/design/TIMELINE.md). A game's timeline is presentation: it may hold
    /// camera, audio, subtitle and event tracks, not tracks that write entities (the game moves
    /// its own). Default: none.
    fn timeline_requests(&mut self, _out: &mut Vec<TimelineRequest>) {}

    /// Something a playing timeline did that the game may act on: its start and end (finished,
    /// skipped by the player, stopped by an agent, refused), its `events` track, its audio cues
    /// (a seam for the engine's audio service) and its subtitle lines. Events, cues and lines
    /// fire on the tick they fall in, so a run with the same timeline at the same ticks sees the
    /// same calls. Default: nothing.
    fn timeline_event(&mut self, _ctx: &mut Ctx, _event: &TimelineEvent) {}

    /// The player's saved settings (docs/design/ACCESSIBILITY.md), read by the engine before
    /// [`Game::start`]: `text` is the settings file (`None`: none yet, unreadable, or settings
    /// are off; `store.problem` says which), `store` where they are kept and who may save them.
    /// Default: nothing.
    fn use_settings(&mut self, _text: Option<&str>, _store: &SettingsStore) {}

    /// The whole settings file to save, if the settings changed since the last call; the engine
    /// writes it off its thread (the latest wins) into the per-user folder. Taken after every
    /// frame, tick and game tool call. A game honours [`SettingsStore::agents_save`]: without
    /// it, changes an agent made (a `play_*` call, `ui_act`'s synthetic frames) are the
    /// session's and never reach the player's file. Default: `None`.
    fn take_settings(&mut self) -> Option<String> {
        None
    }

    /// The player asked to quit (a title screen's Quit, pressed with the window's own input):
    /// the engine stops as when the window's close button is pressed. Never honoured headless;
    /// a game must not ask for an agent (agents don't close the human's window). Taken after
    /// every frame. Default: `false`.
    fn take_quit(&mut self) -> bool {
        false
    }

    /// After every tick's modules ran (the game's own tick came first): what the modules keep
    /// now, to read. A game whose rules are modules decides here how the run stands; it may
    /// write the world as in [`Game::tick`]. Default: nothing.
    fn after_modules(&mut self, _ctx: &mut Ctx, _modules: &ModuleStates) {}

    /// What the modules keep, outside a tick: before a frame is laid out and before the game's
    /// interface is read, so a screen shows a module's state as of now. Read only. Default:
    /// nothing.
    fn see_modules(&mut self, _modules: &ModuleStates) {}

    /// Events for the game's modules, taken right before they tick: each reaches every module
    /// with that tick as an event from `engine` (a press of one of the game's own buttons, a
    /// setting the rules read, the run's end). The only way anything outside the simulation
    /// reaches it, and only at a tick boundary. Default: none.
    fn take_module_events(&mut self, _out: &mut Vec<(String, specula_sdk::Value)>) {}

    /// The game stops (`Some(true)`) or starts the engine's clock, as its own doing: a paused
    /// game does not tick, so nothing counted in ticks runs out behind a menu. Taken after
    /// every frame, tick and call into the game. Default: `None`.
    fn take_clock(&mut self) -> Option<bool> {
        None
    }

    /// The clock was started or stopped by someone else since the game last heard
    /// (`time_control`): told before the next frame or call. Default: nothing.
    fn clock_seen(&mut self, _paused: bool) {}

    /// How the run stands, for `game_state`: its outcome, score, the screen that is up, the
    /// player's settings. One bounded object. `None` (the default): the game does not say.
    fn game_state(&self) -> Option<Json> {
        None
    }

    /// The module folders the game is made of, below the project (with `/`), as its game file
    /// names them: what `--run` builds and loads at start. Default: none.
    fn modules(&self) -> Vec<String> {
        Vec::new()
    }

    /// Something the engine could not do for the game, in words for a person (a module the
    /// start-up build could not load): the game shows it. Default: nothing.
    fn told(&mut self, _problem: &str) {}

    /// `game_act`: a button's word pressed by an agent (`start`, `pause`, `set:volume:3`, a word
    /// of the game's own), as a press on a screen would do it; what only the player may do is
    /// refused. `Err`: a code to branch on, and why. `None` (the default): the game takes no
    /// words (its own tools drive it).
    fn act(&mut self, _ctx: &mut Ctx, _word: &str) -> Option<Result<(), (&'static str, String)>> {
        None
    }

    /// How many presses the game refused since this was last taken because only the player may
    /// make them (an agent pressing Quit, or a button that would listen for the player's next
    /// key). The engine takes it after an agent's `ui_act` or game tool call and tells the input
    /// monitor (seen, never counted: an agent walking a menu presses every button). Default: 0.
    fn take_refused_presses(&mut self) -> u32 {
        0
    }
}

/// What the game's modules keep in their state stores, lent to the hosted game to read
/// ([`Game::after_modules`], [`Game::see_modules`]): the interface shows it and the run's end
/// is decided from it. Never a way to change it: a module's state is its own.
pub struct ModuleStates<'a> {
    host: &'a crate::logic::LogicHost,
}

impl<'a> ModuleStates<'a> {
    pub(crate) fn of(host: &'a crate::logic::LogicHost) -> ModuleStates<'a> {
        ModuleStates { host }
    }

    /// The value module `module` keeps under `key`, while the module is running. What a
    /// module that is not running left behind (unloaded and not forgotten, or disabled after
    /// a failure) is nobody's to decide a run by: it reads as a key that is not there (review
    /// R1 of M6 part 3: a new run was won at once on the last run's count).
    pub fn get(&self, module: &str, key: &str) -> Option<&'a specula_sdk::Value> {
        if !self.host.running(module) {
            return None;
        }
        self.host.state_of(module)?.get(key)
    }

    /// The value a dotted key names: `gate.hp` is module `gate`'s `hp` (the key may itself hold
    /// dots: the module is what stands before the first).
    pub fn dotted(&self, key: &str) -> Option<&'a specula_sdk::Value> {
        let (module, key) = key.split_once('.')?;
        self.get(module, key)
    }

    /// Every running module that keeps state, with what it keeps, by name.
    pub fn each(&self) -> impl Iterator<Item = (&'a str, &'a std::collections::BTreeMap<String, specula_sdk::Value>)> {
        let host = self.host;
        host.states().filter(move |(name, _)| host.running(name))
    }

    /// The names of the modules that are loaded and running.
    pub fn running(&self) -> Vec<String> {
        self.host.modules().into_iter().filter(|m| self.host.running(m)).collect()
    }
}

/// Where the player's settings live ([`Game::use_settings`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsStore {
    /// `per_user` (the operating system's per-user settings folder, the default with a window),
    /// `folder` (the operator's `--settings DIR`), or `off` (headless by default, or
    /// `--settings off`: nothing is read or saved).
    pub kind: &'static str,
    /// Changes agents make are saved too: only in a folder the operator named (scripts, tests),
    /// never in the player's own per-user folder.
    pub agents_save: bool,
    /// Why the file wasn't read (too large, not UTF-8, unreadable), if it wasn't.
    pub problem: Option<String>,
}

impl SettingsStore {
    /// Whether settings are saved at all.
    pub fn saving(&self) -> bool {
        self.kind != "off" && !self.kind.is_empty()
    }

    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([("store", Json::from(if self.kind.is_empty() { "off" } else { self.kind })), ("agents_save", Json::from(self.agents_save))]);
        if let Some(p) = &self.problem {
            j.set("problem", Json::from(p.as_str()));
        }
        j
    }
}

/// Which of a game's interfaces [`Game::ui`] hands out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiPick {
    /// The one the player sees and presses (laid out by window frames, and by `ui_act`'s).
    Live,
    /// [`UiPick::Live`] as the last frame left it, without bringing its state up to date first:
    /// what `ui_act`'s step runner reads right after each of its frames (that frame just filled
    /// it; refilling it again doubled the HUD's work per synthetic frame).
    Peek,
    /// The one the last capture drew ([`Overlay::capture`]), as laid out for it: `ui_frame`'s
    /// outlines come from it. A game that draws captures with its live interface returns that.
    Capture,
}

/// A game's language, for the `l10n_*` tools ([`Game::locale`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameLocale {
    /// The locale its interface shows now (`None`: it has no string table).
    pub locale: Option<String>,
    /// The string-table keys its code may show (its screens' `@key` texts are read from the
    /// screen files): `None` when it doesn't list them, so `l10n_check` can't call a key unused.
    pub keys: Option<Vec<String>>,
}

/// Most keys [`GameLocale::keys`] may list (more are cut).
pub const MAX_GAME_KEYS: usize = 8192;

/// A game's interface lent to the engine's `ui_*` tools.
pub struct GameUi<'a> {
    pub ui: &'a mut specula_ui::Ui,
    /// The state its screens bind to.
    pub state: &'a specula_ui::StateMap,
    /// The player's interface-size setting.
    pub user_scale: f32,
    /// The game's input map (`ui_check` verifies that HUD shortcuts have a gamepad binding).
    pub input_map: Option<&'a specula_input::ActionMap>,
}

/// A game's request to play or stop a timeline ([`Game::timeline_requests`]).
#[derive(Clone, Debug, PartialEq)]
pub enum TimelineRequest {
    /// Plays `path` (a project path such as `assets/timelines/intro.json`) from its start.
    /// `window`: the window looks through its rail camera while it plays; `skippable`: a human's
    /// input ends it (`Ended { how: "skipped" }`); `restore`: the window camera goes back
    /// to where it was when it ends.
    Play { path: String, window: bool, skippable: bool, restore: bool },
    /// Stops `path` if it plays (`Ended { how: "stopped" }`).
    Stop { path: String },
}

/// What a timeline did ([`Game::timeline_event`]).
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineEvent {
    /// The timeline's project path.
    pub path: String,
    /// Its time, seconds.
    pub t: f64,
    pub what: TimelineWhat,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TimelineWhat {
    Started,
    /// An `events` track's event.
    Event { name: String, data: Json },
    /// An audio cue.
    Cue(specula_tempus::Cue),
    /// A subtitle line starting (it stays `line.duration` seconds).
    Subtitle(specula_tempus::Line),
    /// It ended: `finished`, `skipped` (a human's input), `stopped` (an agent or the game),
    /// `replaced` (played again, or an agent's playback the game's own request took over: the
    /// path plays on), `expired` (paused or blocked past the idle lease), `session_closed` (its
    /// starter's last session closed), `world_replaced` (a load) or `refused` (it couldn't start:
    /// `error` says why). Events are by path: an agent's playback of a path the game uses is
    /// heard too.
    Ended { how: &'static str, error: Option<String> },
}

/// A player-facing request a game hands the engine ([`Game::take_requests`]).
#[derive(Clone, Debug, PartialEq)]
pub enum GameRequest {
    /// `save_write` to the save named `slot`, replacing it.
    Save { slot: String, label: Option<String> },
    /// `save_read` of the save named `slot` (discarding unsaved world changes: the player asked).
    Load { slot: String },
    /// `save_list`: the saves there are (newest first), for a load screen.
    List,
    /// `save_delete` of the save named `slot` (the player asked, and confirmed).
    Delete { slot: String },
}

/// [`Game::state_parser`]: reads a game's save bytes on a worker thread into a parsed state for
/// [`Game::apply_state`].
pub type StateParser = Box<dyn FnOnce(&[u8]) -> Result<Box<dyn Any + Send>, String> + Send>;

/// What a game can touch.
///
/// Everything the game changes in the world is journaled under its name (see
/// [`crate::journal`]): per tick (and per frame) the changes are coalesced to one per entity and
/// kept replay-only, so hundreds of updates a tick stay cheap. The game is the simulation's rules,
/// so its writes are never checked against agents' claims.
pub struct Ctx<'a> {
    /// The world, to read in full and write entities and the environment through
    /// ([`GameWorld`]): never the `World` itself.
    pub world: GameWorld<'a>,
    pub clock: &'a Clock,
    /// Attribution for world changes: the game's name.
    pub agent: &'a str,
    /// A human's window is open (false headless): e.g. skip presentation only people see.
    pub windowed: bool,
}

/// The hosted game's handle on a world (docs/design/WRITE_POLICY.md): it reads like a
/// [`World`] (`Deref`) and writes entities and the environment, each write recorded for the
/// engine's write policy to check (claims, for a tool an agent called) and journal. It never
/// hands out the `World` itself: no `DerefMut`, no accessor, so a game can't replace the world,
/// stop it recording or take its recorded changes.
///
/// ```compile_fail
/// fn reset(ctx: &mut specula::game::Ctx) {
///     *ctx.world = specula::world::World::default(); // no DerefMut: refused by the compiler
/// }
/// ```
///
/// ```compile_fail
/// fn hide(ctx: &mut specula::game::Ctx) {
///     ctx.world.take_changes(); // a `World` method that needs `&mut World`: not reachable
/// }
/// ```
pub struct GameWorld<'a> {
    world: &'a mut dyn crate::write_policy::WorldWrite,
}

impl<'a> GameWorld<'a> {
    /// A handle on `world`. The engine makes one for each of the game's calls; a game's own
    /// tests make one over a world they hold.
    pub fn new(world: &'a mut World) -> GameWorld<'a> {
        GameWorld { world }
    }

    /// A handle over a writer's handle (the engine's: the game never sees more than this).
    pub(crate) fn over(world: &'a mut dyn crate::write_policy::WorldWrite) -> GameWorld<'a> {
        GameWorld { world }
    }

    /// Project files aren't the game's to read through the world: they are the project's, read
    /// from disk (see [`Game::assets_changed`]), and the engine lends them out while the game's
    /// own calls run, so this is empty then. Kept only to say so.
    #[deprecated(note = "project files aren't available to the hosted game through the world (empty during its calls): read them from disk, see Game::assets_changed")]
    pub fn files(&self) -> &std::collections::BTreeMap<String, crate::project::ProjectFile> {
        self.world.as_world().files()
    }

    /// A shorter-lived handle on the same world (to hand to a [`Ctx`] or a helper).
    pub fn reborrow(&mut self) -> GameWorld<'_> {
        GameWorld { world: &mut *self.world }
    }

    /// [`World::spawn`].
    pub fn spawn(&mut self, spec: crate::world::SpawnSpec, by: &str) -> Result<&crate::world::Entity, crate::world::WorldError> {
        self.world.spawn(spec, by)
    }

    /// [`World::update`].
    pub fn update(&mut self, key: &crate::world::EntityKey, change: crate::world::UpdateSpec, by: &str) -> Result<&crate::world::Entity, crate::world::WorldError> {
        self.world.update(key, change, by)
    }

    /// [`World::delete`].
    pub fn delete(&mut self, key: &crate::world::EntityKey, expect_version: Option<u64>) -> Result<crate::world::Deleted, crate::world::WorldError> {
        self.world.delete(key, expect_version)
    }

    /// [`World::apply_batch`].
    pub fn apply_batch(&mut self, ops: Vec<crate::world::Op>, by: &str) -> Result<Vec<crate::world::OpResult>, (usize, crate::world::WorldError)> {
        self.world.apply_batch(ops, by)
    }

    /// [`World::set_env`].
    pub fn set_env(&mut self, patch: &crate::world::EnvPatch) -> Result<(), crate::world::WorldError> {
        self.world.set_env(patch)
    }

    /// [`World::set_motion`].
    pub fn set_motion(&mut self, id: u64, position: Option<specula_core::math::Vec3>, rotation_deg: Option<specula_core::math::Vec3>, body: crate::world::Body, by: &str) -> Result<&crate::world::Entity, crate::world::WorldError> {
        self.world.set_motion(id, position, rotation_deg, body, by)
    }
}

impl std::ops::Deref for GameWorld<'_> {
    type Target = World;
    fn deref(&self) -> &World {
        self.world.as_world()
    }
}

impl Ctx<'_> {
    /// Whether the engine clock is paused (`time_control`); ticks don't run while it is.
    pub fn paused(&self) -> bool {
        self.clock.paused()
    }
}

/// Per-frame input and presentation state.
pub struct Frame<'a> {
    /// Window input since the last frame, oldest first (mouse in client pixels, origin top-left).
    pub input: &'a [Event],
    /// Window client size in physical pixels. **0×0 while the window is minimized**: `frame` still
    /// runs then (input such as gamepads keeps arriving), so guard divisions and clamps on it.
    pub size: (u32, u32),
    /// How far between the last tick and the next one (0..1), for smooth motion.
    pub alpha: f32,
    /// Real time since the last frame, in seconds.
    pub dt_s: f32,
    /// The window camera; `None` = auto-frame the world. Agents' `window_view` writes the same slot.
    pub camera: &'a mut Option<Camera>,
    /// The window looks through an agent's camera (`window_view {camera}`): the engine keeps that
    /// view (writes to `camera` are ignored meanwhile), so neither adopt nor fight it. A human's
    /// input ends it; the next frame has this false and `camera` is where the agent's view was:
    /// carry on from there.
    pub agent_view: bool,
    /// Set to replace the window title (the HUD until the engine has text).
    pub title: &'a mut Option<String>,
    /// The gamepads this frame, polled by the engine (empty without XInput). Read pads from here,
    /// not from your own poll: while an engine screen holds the input (the review screen is
    /// modal) they are all released, and a button still held from then stays released until it
    /// is let go (see [`PadGate`]), so the owner's presses there never also act in the game.
    pub pads: &'a [GamepadState],
    /// An agent's synthetic input frame (`ui_act`), not the window's. A game feeds its input with
    /// `specula_input::Input::update_synthetic` for these (applied whatever the window's focus,
    /// never the answer to a rebind) and treats nothing in them as the player's (settings they
    /// change aren't saved; Quit and rebinding are refused). Every world write it makes
    /// commits as the calling agent's, checked against claims, so make only the writes its input
    /// causes (the commands a press sends). Leave out what only presents (interpolating moving
    /// things, hover markers): the window's next frame or a capture's `before_capture` draws
    /// those, as the game's own writes. (Review R1 of m5-uitools, finding 4: interpolation of
    /// another agent's claimed enemies refused a `ui_act {wait}`.)
    pub synthetic: bool,
}

/// Gamepads for the hosted game around an engine screen that takes the input: while `captured`
/// every pad reads released (sticks and triggers centred), and a button held at any moment of
/// the capture stays masked afterwards until it is let go, so the press that closes the screen
/// never arrives in the game as a new press. Allocation-free.
#[derive(Clone, Copy, Debug, Default)]
pub struct PadGate {
    masked: [u32; MAX_GAMEPADS],
}

impl PadGate {
    pub fn apply(&mut self, captured: bool, pads: &mut [GamepadState]) {
        for (p, mask) in pads.iter_mut().zip(self.masked.iter_mut()) {
            let held = p.buttons | p.raw.buttons;
            if captured {
                *mask |= held;
                *p = GamepadState { connected: p.connected, ..GamepadState::default() };
            } else {
                *mask &= held;
                p.buttons &= !*mask;
                p.raw.buttons &= !*mask;
            }
        }
    }
}

/// Keeps the quads a frame can draw: at most `max`, and only those whose texture `live` knows
/// (a game naming a texture it never uploaded, or already freed, would otherwise fail the GPU
/// frame and with it the window). Returns how many were dropped.
pub fn keep_drawable(quads: &mut Vec<OverlayQuad>, max: usize, live: impl Fn(specula_gpu::TextureId) -> bool) -> usize {
    let before = quads.len();
    quads.retain(|q| q.texture.is_none_or(&live));
    quads.truncate(max);
    before - quads.len()
}

/// One image's game interface (see [`Game::overlay`]).
pub struct Overlay<'a> {
    /// Size of the image in physical pixels (never 0×0).
    pub size: (u32, u32),
    /// The camera the image is drawn with (for screen-space markers such as threat arrows).
    pub camera: Camera,
    /// A capture (`sight_frame {include_ui}`), not the window: lay out for `size` and take no
    /// input; the window's next frame lays out for the window again.
    pub capture: bool,
    /// The quads to draw, in order (empty when the call starts). Colours are sRGB.
    pub quads: &'a mut Vec<OverlayQuad>,
    /// Textures for the quads (glyph atlas pages, icons): the game owns the ones it uploads and
    /// frees them. Textures live as long as the engine's GPU does.
    pub textures: &'a mut dyn OverlayTextures,
}

/// Fixed-tick simulation clock: real time in, whole ticks out.
#[derive(Clone, Debug)]
pub struct Clock {
    hz: u32,
    tick: u64,
    paused: bool,
    /// Real-time multiplier (fast-forward or slow motion).
    scale: f32,
    /// Real time not yet turned into ticks, in nanoseconds of simulated time.
    pending_ns: u64,
    /// Ticks requested by `time_step` while paused.
    steps: u64,
}

/// At most this many ticks per frame: a long stall (debugger, a slow load) drops time rather than
/// trying to catch up and stalling again.
const MAX_TICKS_PER_ADVANCE: u64 = 8;

impl Clock {
    pub fn new(hz: u32) -> Clock {
        assert!(hz > 0 && hz <= 1000, "tick rate {hz} out of range");
        Clock { hz, tick: 0, paused: false, scale: 1.0, pending_ns: 0, steps: 0 }
    }

    pub fn hz(&self) -> u32 {
        self.hz
    }

    /// Ticks completed so far: the only time the simulation knows.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Simulated seconds so far (for presentation and animation only).
    pub fn seconds(&self) -> f64 {
        self.tick as f64 / f64::from(self.hz)
    }

    fn tick_ns(&self) -> u64 {
        1_000_000_000 / u64::from(self.hz)
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.pending_ns = 0;
    }

    /// Clamped to 0.05..=64.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = if scale.is_finite() { scale.clamp(0.05, 64.0) } else { 1.0 };
    }

    /// Queues `n` ticks to run while paused.
    pub fn step(&mut self, n: u64) {
        self.steps = self.steps.saturating_add(n);
    }

    /// Drops ticks queued by [`Clock::step`] that haven't run yet (a playtest takes the clock
    /// over: an earlier `time_control step` must not run on into it).
    pub fn cancel_steps(&mut self) {
        self.steps = 0;
    }

    /// Adds real time; returns how many ticks to run now.
    pub fn advance(&mut self, real: Duration) -> u64 {
        if self.paused {
            let n = self.steps.min(MAX_TICKS_PER_ADVANCE * 64);
            self.steps -= n;
            return n;
        }
        let scaled = (real.as_nanos() as f64 * f64::from(self.scale)) as u64;
        self.pending_ns = self.pending_ns.saturating_add(scaled);
        // Fast-forward needs proportionally more ticks per frame.
        let max = MAX_TICKS_PER_ADVANCE * (self.scale.ceil() as u64).max(1);
        let n = (self.pending_ns / self.tick_ns()).min(max);
        self.pending_ns = if n == max { 0 } else { self.pending_ns - n * self.tick_ns() };
        n
    }

    /// Jumps to `tick` (a save game's): the rate, pause and scale stay; time not yet turned
    /// into ticks is dropped.
    pub fn set_tick(&mut self, tick: u64) {
        self.tick = tick;
        self.pending_ns = 0;
        self.steps = 0;
    }

    /// Marks one tick as done (call after the game's `tick`).
    pub fn finish_tick(&mut self) {
        self.tick += 1;
    }

    /// Fraction of the next tick already elapsed (0 while paused).
    pub fn alpha(&self) -> f32 {
        if self.paused {
            0.0
        } else {
            (self.pending_ns as f64 / self.tick_ns() as f64).clamp(0.0, 1.0) as f32
        }
    }

    /// Real time until the next tick is due (headless engines sleep this long at most).
    pub fn until_next(&self) -> Duration {
        if self.paused {
            return Duration::from_millis(100);
        }
        let left = self.tick_ns().saturating_sub(self.pending_ns);
        Duration::from_nanos((left as f64 / f64::from(self.scale)) as u64)
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("tick", Json::from(self.tick)),
            ("tick_hz", Json::from(self.hz)),
            ("seconds", Json::from(self.seconds())),
            ("paused", Json::from(self.paused)),
            ("scale", Json::from(self.scale)),
        ])
    }
}

#[cfg(test)]
mod pad_gate_tests {
    use super::*;

    #[test]
    fn captured_pads_read_released_until_let_go() {
        let south = specula_platform::GamepadButton::South.bit();
        let held = |b: u32| {
            let mut p = GamepadState { connected: true, buttons: b, left_stick: [0.5, 0.0], ..GamepadState::default() };
            p.raw.buttons = b;
            p
        };
        let mut gate = PadGate::default();
        let mut pads = [held(south)];
        gate.apply(true, &mut pads);
        assert_eq!((pads[0].buttons, pads[0].raw.buttons, pads[0].left_stick, pads[0].connected), (0, 0, [0.0; 2], true));
        // Still held after the screen closed: masked (no new press for the game).
        let mut pads = [held(south)];
        gate.apply(false, &mut pads);
        assert_eq!(pads[0].buttons, 0);
        assert_eq!(pads[0].left_stick, [0.5, 0.0], "sticks pass once the capture ends");
        // Let go, then pressed again: the game sees it.
        let mut pads = [held(0)];
        gate.apply(false, &mut pads);
        let mut pads = [held(south)];
        gate.apply(false, &mut pads);
        assert_eq!(pads[0].buttons, south);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_time_becomes_whole_ticks() {
        let mut c = Clock::new(30);
        assert_eq!(c.advance(Duration::from_millis(20)), 0);
        assert_eq!(c.advance(Duration::from_millis(20)), 1); // 40 ms ≥ 33.3 ms
        assert!(c.alpha() > 0.15 && c.alpha() < 0.25, "{}", c.alpha());
        assert_eq!(c.advance(Duration::from_millis(100)), 3);
    }

    #[test]
    fn long_stalls_drop_time_instead_of_spiralling() {
        let mut c = Clock::new(30);
        assert_eq!(c.advance(Duration::from_secs(10)), MAX_TICKS_PER_ADVANCE);
        assert_eq!(c.advance(Duration::from_millis(1)), 0);
    }

    #[test]
    fn pause_step_and_scale() {
        let mut c = Clock::new(30);
        c.set_paused(true);
        assert_eq!(c.advance(Duration::from_secs(1)), 0);
        c.step(3);
        assert_eq!(c.advance(Duration::ZERO), 3);
        assert_eq!(c.advance(Duration::ZERO), 0);
        c.set_paused(false);
        c.set_scale(4.0);
        assert_eq!(c.advance(Duration::from_millis(100)), 12);
        c.set_scale(f32::NAN);
        assert_eq!(c.scale(), 1.0);
    }

    #[test]
    fn game_quads_with_unknown_textures_or_past_the_cap_are_dropped() {
        use specula_gpu::TextureId;
        let solid = OverlayQuad::default();
        let tex = |id| OverlayQuad { texture: Some(TextureId(id)), ..OverlayQuad::default() };
        let mut q = vec![solid, tex(1), tex(2), solid, tex(1)];
        assert_eq!(keep_drawable(&mut q, 10, |t| t.0 == 1), 1, "texture 2 is unknown");
        assert_eq!(q.len(), 4);
        assert_eq!(keep_drawable(&mut q, 3, |_| true), 1, "the cap");
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn ticks_count_up() {
        let mut c = Clock::new(20);
        for _ in 0..c.advance(Duration::from_millis(100)) {
            c.finish_tick();
        }
        assert_eq!((c.tick(), c.seconds()), (2, 0.1));
    }
}

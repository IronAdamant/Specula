//! [`SepulcraGame`]: the simulation hosted by the engine.
//!
//! **Time.** One engine tick steps the simulation once (or 2–4 times at fast speed). Player and
//! agent commands are applied the moment they arrive with [`State::apply`] and journaled with the
//! tick they were applied at; the next tick then steps with no commands. That is exactly what
//! the headless runner does (`sepulcra::run`), so the same seed and the same commands at the same
//! ticks give the same [`State::hash`] (foundation F2), and building works during a pause.
//!
//! **Pause.** The engine's clock belongs to the engine (agents drive it with `time_control`); the
//! game can't pause it, so P pauses the *game*: ticks keep coming but the simulation isn't
//! stepped (and creatures' clips stand still).
//!
//! **Presentation** is the [`Scene`] (the world's entities), the [`Hud`] (the GPU overlay) and
//! [`Sfx`] (sound): all three read the simulation and never write it.

use crate::audio::Sfx;
use crate::controls::{Action, Controls};
use crate::hud::{self, Hud, HudInfo, Inspect, UI_SCALES};
use crate::scene::{CursorLook, Scene};
use crate::{looks::palette, tools};
use sepulcra::map::Face;
use sepulcra::script::{Journal, Player};
use sepulcra::sim::{DustSource, Stone, Wall};
use sepulcra::{CellId, Command, Content, Event, Motion, Outcome, Rejection, State, TICK_HZ};
use specula::effects::{BloomPatch, FogPatch, GradingFields, GradingPatch, ShadowPatch, SkyPatch, SsaoPatch, WindPatch};
use specula::game::{Ctx, Frame, Game, Overlay, GameWorld};
use specula::sight::Camera;
use specula::world::EnvPatch;
use specula_core::math::Vec3;
use specula_core::Json;
use specula_input::PadStyle;
use specula_protocol::{ToolDef, ToolOutput};
use specula_ui::{UiEvent, UiEvents};
use std::any::Any;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "saves.rs"]
mod saves;
pub use saves::{MAX_RECORDING_BYTES, MAX_STATE_BYTES};

/// A window frame this recent means another follows the next tick.
const FRAMED_WITHIN: Duration = Duration::from_millis(250);
const LOG_LEN: usize = 32;
pub const SPEEDS: [u32; 3] = [1, 2, 4];
/// Button-prompt styles the settings cycle through (`Auto` follows the pad).
const GLYPHS: [(&str, Option<PadStyle>); 4] =
    [("Auto", None), ("Xbox", Some(PadStyle::Xbox)), ("PlayStation", Some(PadStyle::PlayStation)), ("Steam Deck", Some(PadStyle::SteamDeck))];

/// What the open confirmation dialog will do on "yes".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Confirm {
    Restart,
    /// The Lex rules and the reference disagreed: the run stopped ("yes" starts it again).
    Desync,
    /// The saves screen: save over player slot N, load a slot, delete player slot N.
    Overwrite(u8),
    Load(saves::Slot),
    Delete(u8),
}

/// The game, as the engine hosts it.
pub struct SepulcraGame {
    pub(crate) state: State,
    /// Every applied command and its tick: the run's recording.
    pub(crate) journal: Journal,
    /// A build order being followed (`play_new` with a layout).
    pub(crate) autoplay: Option<(String, Player)>,
    pub(crate) scene: Option<Scene>,
    controls: Controls,
    pub(crate) hud: Hud,
    pub(crate) sfx: Sfx,
    /// Stone index, or `None` for the wall.
    selected: Option<u8>,
    /// The stone or wall whose panel is open.
    pub(crate) inspect: Option<CellId>,
    /// Where the radial build menu builds (the cell under the cursor when it opened).
    radial_cell: Option<CellId>,
    confirm: Option<Confirm>,
    pub(crate) paused: bool,
    /// Paused because an engine screen (review, gallery) holds the window; resumed when it
    /// closes. Cleared by any other pause or resume, a new run and a load (taste review 2: an
    /// agent's explicit pause was undone when the screen closed).
    pub(crate) modal_paused: bool,
    pub(crate) speed: u32,
    events: Vec<Event>,
    /// Events from commands applied between ticks (flashes on the next sync).
    pending: Vec<Event>,
    /// Presentation time (flashes, bobbing); stops while paused.
    now: f32,
    /// Presentation ticks: engine ticks while the game isn't paused (times the speed). Flashes
    /// and bobbing run on these, so they freeze with the clock and replay the same under
    /// `time_control step` as in real time.
    pres_ticks: u64,
    /// When `frame` last ran (window frames follow ticks, so ticks needn't move things too).
    last_frame: Option<Instant>,
    /// The hovered cell and what stood there when its text was made.
    hover_key: Option<(CellId, Option<Stone>, Option<Wall>)>,
    hover_text: String,
    /// Recent messages with their tick, for agents (`play_state`).
    pub(crate) log: VecDeque<(u32, String)>,
    /// The seed `configure` was given (`--seed`), for the first run.
    seed_arg: Option<u64>,
    /// `--mute`: no sound or music, whatever the engine's audio is.
    mute: bool,
    /// The engine's audio service (`Game::use_audio`); off headless unless `--audio null`.
    engine_audio: Option<Arc<specula::audio_service::AudioService>>,
    /// Where the game's assets are (recipes, anim sets, sounds, screens).
    assets: Option<PathBuf>,
    /// Settings (the settings screen).
    ui_scale: usize,
    volume: f32,
    reduced_motion: bool,
    glyphs: usize,
    /// The button-prompt style before the settings overrode it ("Auto").
    auto_glyphs: Option<PadStyle>,
    /// The window camera last frame (threat arrows, listener).
    camera: Option<Camera>,
    /// The title last sent.
    title_sent: bool,
    /// The HUD's strings, kept between frames.
    hud_texts: hud::Texts,
    /// This frame's HUD events (storage reused).
    ui_frame: UiEvents,
    /// Scratch for the braziers' crackle (reused).
    fire_buf: Vec<(CellId, Vec3)>,
    /// `--rules lex`: the Lex rules decide, the state above is their reference (see
    /// [`crate::lexmode`]). `rules_lex` keeps the choice across new runs.
    pub(crate) lex: Option<crate::lexmode::LexRules>,
    pub(crate) rules_lex: bool,
    /// The pause menu's Save and Load, for the engine (see `saves`).
    requests: Vec<specula::game::GameRequest>,
    /// The player's slots as the last save_list showed them (the saves screen).
    slots: Vec<saves::SlotInfo>,
    /// "Saving…" / "Loading…" while a request runs.
    saves_status: Option<String>,
    /// Autosave at the start of each wave (Settings; on by default with a window).
    autosave_on: bool,
    /// The intro flyover and the victory shot (timelines the engine plays; see `cutscene`).
    pub(crate) cutscene: crate::cutscene::Cutscenes,
}

/// Everything a game tool can change, for [`Game::checkpoint`]: the simulation, the recording,
/// the scene with its cached entity ids, and what the player sees.
struct Checkpoint {
    state: State,
    journal: Journal,
    autoplay: Option<(String, Player)>,
    scene: Option<Scene>,
    selected: Option<u8>,
    inspect: Option<CellId>,
    paused: bool,
    speed: u32,
    events: Vec<Event>,
    pending: Vec<Event>,
    now: f32,
    pres_ticks: u64,
    log: VecDeque<(u32, String)>,
    /// What the player sees of the HUD: open screens, a pending confirmation, the radial's cell.
    screens: Vec<String>,
    confirm: Option<Confirm>,
    radial_cell: Option<CellId>,
    lex: Option<crate::lexmode::LexRules>,
}

impl SepulcraGame {
    pub fn new(content: Arc<Content>, seed: u64) -> SepulcraGame {
        let controls = Controls::default();
        // The real HUD loads at start (fonts, screens); until then it's off.
        let hud = Hud::off("not started");
        SepulcraGame {
            state: State::new(content, seed),
            journal: Journal::new(),
            autoplay: None,
            scene: None,
            controls,
            hud,
            sfx: Sfx::off("not started"),
            selected: Some(0),
            inspect: None,
            radial_cell: None,
            confirm: None,
            paused: false,
            modal_paused: false,
            speed: 1,
            events: Vec::new(),
            pending: Vec::new(),
            now: 0.0,
            pres_ticks: 0,
            last_frame: None,
            hover_key: None,
            hover_text: String::new(),
            log: VecDeque::new(),
            seed_arg: None,
            mute: false,
            engine_audio: None,
            assets: None,
            ui_scale: 2,
            volume: 0.8,
            reduced_motion: false,
            glyphs: 0,
            auto_glyphs: None,
            camera: None,
            title_sent: false,
            ui_frame: UiEvents::default(),
            hud_texts: hud::Texts::default(),
            fire_buf: Vec::new(),
            lex: None,
            rules_lex: false,
            requests: Vec::new(),
            slots: Vec::new(),
            saves_status: None,
            autosave_on: false,
            cutscene: crate::cutscene::Cutscenes::default(),
        }
    }

    /// Starts the Lex rules for the current state (`--rules lex`), or says why not.
    fn start_lex(&mut self) {
        self.lex = None;
        if !self.rules_lex {
            return;
        }
        match crate::lexmode::LexRules::new(self.state.content(), self.state.seed()) {
            Ok(l) => self.lex = Some(l),
            Err(e) => self.say(format!("Lex rules off: {e}")),
        }
    }

    /// Whether the run goes on: with the Lex rules on, by their outcome (and a desync stops it).
    pub(crate) fn running(&self) -> bool {
        match &self.lex {
            Some(l) => l.outcome().is_none() && l.desync.is_none(),
            None => self.state.outcome() == Outcome::Running,
        }
    }

    /// The outcome the game shows and acts on: the Lex rules' with them on.
    pub(crate) fn outcome(&self) -> Outcome {
        self.lex.as_ref().map_or(self.state.outcome(), |l| l.result())
    }

    /// (ended, desynced) by the Lex rules, to see what a step or a command changed.
    fn lex_flags(&self) -> (bool, bool) {
        self.lex.as_ref().map_or((false, false), |l| (l.outcome().is_some(), l.desync.is_some()))
    }

    /// With the Lex rules on, they end the run: the reference's own `Won`/`Lost` give way to
    /// theirs (in `self.events`), and a desync stops the game with a dialog saying why.
    fn lex_verdicts(&mut self, was: (bool, bool)) {
        let Some(lex) = &self.lex else { return };
        let (result, desync) = (lex.result(), lex.desync.clone());
        self.events.retain(|e| !matches!(e, Event::Won | Event::Lost));
        if result != Outcome::Running && !was.0 {
            self.events.push(if result == Outcome::Won { Event::Won } else { Event::Lost });
        }
        if let (Some((tick, _)), false) = (desync, was.1) {
            self.confirm = Some(Confirm::Desync);
            self.hud.show("pause", false);
            self.hud.show("confirm", true);
            self.say(format!("Rules desync at tick {tick}: the run stopped (play_state has what differed)"));
        }
    }

    /// Where the game's assets live: `assets/` next to the executable when it was shipped
    /// with one, else the source tree's (`games/sepulcra/assets`).
    pub fn default_assets() -> PathBuf {
        let shipped = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|d| d.join("assets")));
        match shipped {
            Some(dir) if dir.join("recipes").is_dir() => dir,
            _ => {
                let app = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
                app.parent().unwrap_or(app).join("sepulcra").join("assets")
            }
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// The seed the run uses (`--seed`, or `play_new`'s).
    pub fn seed(&self) -> u64 {
        self.state.seed()
    }

    /// Applies a command now (at the current tick) and journals it.
    pub fn command(&mut self, world: &mut GameWorld<'_>, cmd: Command) -> Result<(), Rejection> {
        let tick = self.state.tick();
        // With the Lex rules on, they decide (the state is their reference).
        if let Some(lex) = &mut self.lex {
            let was = (lex.outcome().is_some(), lex.desync.is_some());
            let verdict = lex.decide(&self.state, &cmd);
            if verdict.is_err() {
                // A disagreement stops the game here (the dialog shows at once).
                let events = std::mem::take(&mut self.events);
                self.lex_verdicts(was);
                self.events = events;
            }
            verdict?;
        }
        let before = self.pending.len();
        self.state.apply(&cmd, &mut self.pending)?;
        self.journal.push((tick, cmd));
        // Calling a wave early starts it here, not in a tick.
        if let Some(wave) = wave_started(&self.pending[before..]) {
            self.autosave(wave);
        }
        self.messages_from_pending();
        self.sounds(&self.pending.clone());
        // Show it at once, even while the engine clock is paused.
        if let Some(scene) = &mut self.scene {
            scene.sync_structure(world, &self.state, &self.pending, self.now);
            self.pending.clear();
        }
        Ok(())
    }

    /// Starts over: a fresh state (and slab) for `content` and `seed`, optionally following a
    /// build order.
    pub fn restart(&mut self, world: &mut GameWorld<'_>, agent: &str, content: Arc<Content>, seed: u64, autoplay: Option<(String, Player)>) {
        self.state = State::new(content, seed);
        self.start_lex();
        self.journal.clear();
        self.autoplay = autoplay;
        self.events.clear();
        self.pending.clear();
        self.log.clear();
        self.paused = false;
        self.modal_paused = false;
        self.inspect = None;
        self.sfx.reset();
        for screen in ["pause", "victory", "defeat", "radial", "confirm"] {
            self.hud.show(screen, false);
        }
        self.rebuild(world, agent);
        self.say(format!("New game, seed {seed}"));
    }

    /// Replaces the state (a loaded replay) and rebuilds the scene.
    pub(crate) fn load(&mut self, world: &mut GameWorld<'_>, agent: &str, state: State, journal: Journal) {
        // The Lex rules replay the same commands to the same tick.
        if self.rules_lex {
            self.lex = match crate::lexmode::LexRules::replayed(state.content(), state.seed(), &journal, state.tick()) {
                Ok(mut l) => {
                    l.check(&state);
                    Some(l)
                }
                Err(e) => {
                    self.say(format!("Lex rules off: the replay doesn't run under them ({e})"));
                    None
                }
            };
        }
        self.state = state;
        self.journal = journal;
        self.autoplay = None;
        self.events.clear();
        self.pending.clear();
        self.inspect = None;
        self.rebuild(world, agent);
    }

    fn rebuild(&mut self, world: &mut GameWorld<'_>, agent: &str) {
        self.hover_key = None;
        let mut scene = Scene::build(world, agent, &self.state);
        scene.set_pace(world, self.pace());
        scene.sync(world, &self.state, &[], self.now);
        scene.animate(world, 1.0, self.now);
        self.scene = Some(scene);
    }

    /// How fast creatures move: 0 while the game is paused, else its speed.
    fn pace(&self) -> f32 {
        if self.paused {
            0.0
        } else {
            self.speed as f32
        }
    }

    /// Pauses or resumes the game (the P key, the HUD's button, `play_control`).
    /// Active pause (DESIGN.md): the dead stand still, building still works. The pause *menu* is
    /// [`SepulcraGame::open_menu`].
    pub(crate) fn set_paused(&mut self, world: &mut GameWorld<'_>, paused: bool) {
        // Anyone else's pause or resume is theirs to keep: the screen no longer resumes it.
        self.modal_paused = false;
        self.apply_pause(world, paused);
    }

    /// The pause itself (the modal screen's, which remembers that it paused).
    fn apply_pause(&mut self, world: &mut GameWorld<'_>, paused: bool) {
        self.paused = paused;
        if !paused {
            self.hud.show("pause", false);
        }
        let pace = self.pace();
        if let Some(scene) = &mut self.scene {
            scene.set_pace(world, pace);
        }
    }

    /// The pause menu (Escape, the pad's Start): pauses the game under it.
    pub(crate) fn open_menu(&mut self, world: &mut GameWorld<'_>) {
        if self.running() {
            self.set_paused(world, true);
            self.hud.show("pause", true);
        }
    }

    pub(crate) fn set_speed(&mut self, world: &mut GameWorld<'_>, speed: u32) {
        self.speed = speed;
        let pace = self.pace();
        if let Some(scene) = &mut self.scene {
            scene.set_pace(world, pace);
        }
    }

    pub(crate) fn say(&mut self, msg: String) {
        self.hud.toast(&msg, hud::toast_kind(&msg), self.now);
        if self.log.len() == LOG_LEN {
            self.log.pop_front();
        }
        self.log.push_back((self.state.tick(), msg));
    }

    fn messages_from_pending(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        self.messages(&pending);
        self.pending = pending;
    }

    fn messages(&mut self, events: &[Event]) {
        let content = Arc::clone(self.state.content_arc());
        let map = &content.map;
        for ev in events {
            let msg = match *ev {
                Event::WaveStarted { wave, early } => {
                    format!("Wave {wave} started{}", if early { " early" } else { "" })
                }
                Event::WaveCleared { wave } => format!("Wave {wave} cleared"),
                Event::TombstonesLost { kind, lost, remaining, .. } => {
                    format!("{} raised {lost} dead: {remaining} tombstones left", content.enemy(kind).name)
                }
                Event::WallBroken { cell } => format!("Wall at {} broken", map.cell_name(cell)),
                Event::DustGained { amount, source: DustSource::EarlyCall } => format!("+{amount} dust for calling early"),
                Event::StoneSold { cell, refund } => format!("Sold the stone at {} (+{refund})", map.cell_name(cell)),
                Event::WallRemoved { cell, refund } => format!("Removed the wall at {} (+{refund})", map.cell_name(cell)),
                Event::Won => {
                    self.hud.show("pause", false);
                    self.hud.show("victory", true);
                    "Victory! The graveyard holds.".into()
                }
                Event::Lost => {
                    self.hud.show("pause", false);
                    self.hud.show("defeat", true);
                    "Defeat: the dead walk.".into()
                }
                _ => continue,
            };
            self.say(msg);
        }
    }

    /// Sounds for events (where they happen on the slab).
    fn sounds(&mut self, events: &[Event]) {
        if !self.sfx.enabled() {
            return;
        }
        let content = Arc::clone(self.state.content_arc());
        let Some(scene) = &self.scene else { return };
        let geo = &scene.geo;
        let state = &self.state;
        let anchor = |c: CellId| geo.anchor(&content.map, c).0;
        let stone_at = |c: CellId| state.stone(c).map(|s| s.kind.0);
        self.sfx.events(events, &content, self.now, &anchor, &stone_at);
    }

    pub(crate) fn content(&self) -> Arc<Content> {
        Arc::clone(self.state.content_arc())
    }

    fn build_command(&self, cell: CellId) -> Command {
        let content = self.state.content();
        match self.selected.filter(|&i| usize::from(i) < content.stones.len()) {
            Some(i) => Command::PlaceStone { cell, stone: sepulcra::StoneKind(i) },
            None => Command::PlaceWall { cell },
        }
    }

    /// Why a command was refused, for people.
    pub(crate) fn explain(&self, cmd: &Command, r: Rejection) -> String {
        let content = self.state.content();
        let map = &content.map;
        let what = match *cmd {
            Command::PlaceStone { cell, stone } => {
                format!("place {} at {}", content.stones.get(usize::from(stone.0)).map_or("?", |d| d.name.as_str()), map.cell_name(cell))
            }
            Command::Upgrade { cell } => format!("upgrade at {}", map.cell_name(cell)),
            Command::Sell { cell } => format!("sell at {}", map.cell_name(cell)),
            Command::PlaceWall { cell } => format!("wall {}", map.cell_name(cell)),
            Command::RemoveWall { cell } => format!("remove the wall at {}", map.cell_name(cell)),
            Command::CallWave => "call the next wave".into(),
        };
        let hint = match r {
            Rejection::NotEnoughDust { need, have } => format!("need {need} dust, have {have}"),
            Rejection::Occupied => "a stone is there (select it to upgrade)".into(),
            Rejection::NotBuildable => "stones go on raised fragments and cliff ledges".into(),
            // "a grave angel stands here": the prop's own words, from the map data.
            Rejection::PropBlocks { prop } => map.props.get(usize::from(prop)).map_or_else(|| "a prop stands here".into(), |p| format!("{} stands here", p.what)),
            Rejection::NotWallable => "walls go on top-face lanes".into(),
            Rejection::EnemyInTheWay { .. } => "an enemy is in the way".into(),
            Rejection::MaxTier => "already tier III".into(),
            r => r.code().replace('_', " "),
        };
        format!("Can't {what}: {hint}")
    }

    /// Plays a command from the player (mouse, pad, touch, HUD): refusals are said and buzzed.
    fn try_command(&mut self, world: &mut GameWorld<'_>, cmd: Command) -> bool {
        match self.command(world, cmd) {
            Ok(()) => true,
            Err(r) => {
                let msg = self.explain(&cmd, r);
                self.say(msg);
                self.sfx.ui("ui_error", self.now);
                false
            }
        }
    }

    fn act(&mut self, world: &mut GameWorld<'_>, agent: &str, action: Action) {
        let n = self.state.content().stones.len() as u8;
        let cmd = match action {
            Action::Select(i) => {
                self.selected = i.filter(|&i| i < n).or(if i.is_some() { self.selected } else { None });
                self.inspect = None;
                return;
            }
            Action::SelectNext => {
                self.selected = match self.selected {
                    Some(i) if i + 1 < n => Some(i + 1),
                    Some(_) => None,
                    None => Some(0),
                };
                return;
            }
            Action::BuildMenu => {
                if self.hud.enabled() {
                    self.radial_cell = self.controls.hover;
                    self.hud.show("radial", true);
                } else {
                    self.act(world, agent, Action::SelectNext);
                }
                return;
            }
            Action::Build(cell) => {
                // On a stone or a wall: select it (its panel upgrades, sells, removes).
                if self.state.stone(cell).is_some() || self.state.wall(cell).is_some() {
                    self.inspect = Some(cell);
                    self.sfx.ui("ui_click", self.now);
                    return;
                }
                self.inspect = None;
                self.build_command(cell)
            }
            Action::Upgrade(cell) => Command::Upgrade { cell },
            Action::Sell(cell) => Command::Sell { cell },
            Action::Unwall(cell) => Command::RemoveWall { cell },
            Action::CallWave => Command::CallWave,
            Action::TogglePause => {
                self.set_paused(world, !self.paused);
                return;
            }
            Action::Menu => {
                self.open_menu(world);
                return;
            }
            Action::Cancel => {
                if self.inspect.take().is_none() {
                    self.open_menu(world);
                }
                return;
            }
            Action::ToggleHelp => {
                let open = !self.hud.visible("controls");
                self.hud.show("controls", open);
                return;
            }
            Action::CycleSpeed => {
                let i = SPEEDS.iter().position(|&s| s == self.speed).unwrap_or(0);
                self.set_speed(world, SPEEDS[(i + 1) % SPEEDS.len()]);
                return;
            }
            Action::NewGame => {
                if !self.running() {
                    self.new_run(world, agent);
                } else {
                    self.say("The run is still going (N starts a new game after the end)".into());
                }
                return;
            }
        };
        self.try_command(world, cmd);
    }

    fn new_run(&mut self, world: &mut GameWorld<'_>, agent: &str) {
        let seed = self.state.seed().wrapping_add(1);
        let content = self.content();
        self.restart(world, agent, content, seed, None);
    }

    /// What the HUD's buttons and menus asked for.
    fn ui_events(&mut self, world: &mut GameWorld<'_>, agent: &str, events: &UiEvents) {
        for ev in &events.events {
            match ev {
                UiEvent::Action { action, .. } => {
                    self.sfx.ui("ui_click", self.now);
                    self.ui_action(world, agent, action);
                }
                UiEvent::Refused { .. } => {
                    self.sfx.ui("ui_error", self.now);
                    self.say("Not enough dust".into());
                }
                UiEvent::Back { .. } => {}
            }
        }
    }

    /// One HUD action (also what agents press with `play_ui`).
    pub(crate) fn ui_action(&mut self, world: &mut GameWorld<'_>, agent: &str, action: &str) {
        let content = self.content();
        let stone_of = |icon: &str| content.stones.iter().position(|s| hud::icon_of(&s.id) == icon).map(|i| i as u8);
        match action {
            "call_wave" => {
                self.try_command(world, Command::CallWave);
            }
            "pause" => self.set_paused(world, true),
            "open_menu" => self.open_menu(world),
            "resume" => self.set_paused(world, false),
            "speed" => self.act(world, agent, Action::CycleSpeed),
            "open_build_menu" => self.act(world, agent, Action::BuildMenu),
            "close_build_menu" => {
                self.hud.show("radial", false);
                self.radial_cell = None;
            }
            "upgrade" | "sell" | "unwall" => {
                let Some(cell) = self.inspect else { return };
                let cmd = match action {
                    "upgrade" => Command::Upgrade { cell },
                    "sell" => Command::Sell { cell },
                    _ => Command::RemoveWall { cell },
                };
                if self.try_command(world, cmd) && action != "upgrade" {
                    self.inspect = None;
                }
            }
            "deselect" => self.inspect = None,
            "open_settings" => self.hud.show("settings", true),
            "close_settings" => self.hud.show("settings", false),
            "open_controls" => self.hud.show("controls", true),
            "close_controls" => self.hud.show("controls", false),
            "cycle_ui_scale" => {
                self.ui_scale = (self.ui_scale + 1) % UI_SCALES.len();
                self.hud.user_scale = UI_SCALES[self.ui_scale];
            }
            "cycle_volume" => {
                let v = ((self.volume * 4.0).round() as i32 + 1) % 5;
                self.volume = v as f32 / 4.0;
                self.sfx.set_volume(self.volume);
            }
            "cycle_glyphs" => {
                // "Auto" goes back to the style the input had before any override.
                let input = self.controls.input_mut();
                let auto = *self.auto_glyphs.get_or_insert(input.pad_style());
                self.glyphs = (self.glyphs + 1) % GLYPHS.len();
                input.set_pad_style(GLYPHS[self.glyphs].1.unwrap_or(auto));
            }
            "toggle_motion" => self.reduced_motion = !self.reduced_motion,
            "restart" => {
                self.confirm = Some(Confirm::Restart);
                self.hud.show("confirm", true);
            }
            "confirm_yes" => {
                self.hud.show("confirm", false);
                let asked = self.confirm.take();
                if let Some(c) = asked.filter(|c| !matches!(c, Confirm::Restart | Confirm::Desync)) {
                    self.confirmed(c);
                } else if matches!(asked, Some(Confirm::Restart | Confirm::Desync)) {
                    self.hud.show("pause", false);
                    let (seed, content) = (self.state.seed(), self.content());
                    self.restart(world, agent, content, seed, None);
                }
            }
            "confirm_no" => {
                self.hud.show("confirm", false);
                self.confirm = None;
            }
            "new_run" => {
                self.hud.show("victory", false);
                self.hud.show("defeat", false);
                self.new_run(world, agent);
            }
            "open_saves" => self.open_saves(),
            "close_saves" => self.close_saves(),
            "toggle_autosave" => self.autosave_on = !self.autosave_on,
            a if a.contains(':') && ["save_slot", "load_slot", "delete_slot", "load_auto"].iter().any(|p| a.starts_with(p)) => {
                let (verb, n) = a.split_once(':').unwrap_or((a, ""));
                self.saves_action(verb, n.parse().unwrap_or(0));
            }
            "close_result" => {
                self.hud.show("victory", false);
                self.hud.show("defeat", false);
            }
            a if a.starts_with("select:") || a.starts_with("build:") => {
                let icon = a.split_once(':').map_or("", |(_, i)| i);
                self.selected = if icon == "wall" { None } else { stone_of(icon).or(self.selected) };
                if a.starts_with("build:") {
                    self.hud.show("radial", false);
                    if let Some(cell) = self.radial_cell.take() {
                        if self.state.stone(cell).is_none() && self.state.wall(cell).is_none() {
                            let cmd = self.build_command(cell);
                            self.try_command(world, cmd);
                        }
                    }
                }
                self.inspect = None;
            }
            other => eprintln!("sepulcra: unhandled HUD action '{other}'"),
        }
    }

    fn hover_text(&self, cell: CellId) -> String {
        let content = self.state.content();
        let map = &content.map;
        let mut t = map.cell_name(cell);
        if let Some(s) = self.state.stone(cell) {
            let def = content.stone(s.kind);
            t.push_str(&format!(" · {} {}", def.name, ["I", "II", "III"][usize::from(s.tier.min(2))]));
        } else if let Some(w) = self.state.wall(cell) {
            t.push_str(&format!(" · wall {}/{} HP", w.hp / 1000, w.max_hp / 1000));
        } else if self.state.is_rooted(cell) {
            t.push_str(" · rooted (slows the dead)");
        } else if let Some(p) = map.blocking_prop(cell).and_then(|i| map.props.get(usize::from(i))) {
            t.push_str(&format!(" · {} (no building here)", p.what));
        }
        t
    }

    /// Cursor and range ring for the hovered (or selected) cell.
    fn hover(&mut self, world: &mut GameWorld<'_>) {
        let Some(scene) = self.scene.as_mut() else { return };
        let content = Arc::clone(self.state.content_arc());
        let tier_range = |s: &Stone| content.stone(s.kind).tiers.get(usize::from(s.tier)).map(|t| t.range as f32 / 1000.0);
        // A selected stone keeps its ring and mark while its panel is open.
        if let Some(cell) = self.inspect {
            if self.state.stone(cell).is_none() && self.state.wall(cell).is_none() {
                self.inspect = None;
            } else {
                scene.show_cursor(world, &self.state, CursorLook::Selected(cell));
                scene.show_range(world, &self.state, self.state.stone(cell).and_then(|s| tier_range(s).map(|r| (cell, r))));
                return;
            }
        }
        let Some(cell) = self.controls.hover else {
            scene.show_cursor(world, &self.state, CursorLook::Hidden);
            scene.show_range(world, &self.state, None);
            self.hover_text.clear();
            self.hover_key = None;
            return;
        };
        let cmd = match self.selected.filter(|&i| usize::from(i) < content.stones.len()) {
            Some(i) => Command::PlaceStone { cell, stone: sepulcra::StoneKind(i) },
            None => Command::PlaceWall { cell },
        };
        let occupied = self.state.stone(cell).is_some() || self.state.wall(cell).is_some();
        let ok = occupied || self.state.check(&cmd).is_ok();
        scene.show_cursor(world, &self.state, if ok { CursorLook::Valid(cell) } else { CursorLook::Invalid(cell) });
        let ring = match (self.state.stone(cell), cmd) {
            (Some(s), _) => tier_range(s).map(|r| (cell, r)),
            (None, Command::PlaceStone { stone, .. }) if Scene::buildable(&self.state, cell) => Some((cell, content.stone(stone).tiers[0].range as f32 / 1000.0)),
            _ => None,
        };
        scene.show_range(world, &self.state, ring);
        let key = (cell, self.state.stone(cell).copied(), self.state.wall(cell).copied());
        if self.hover_key != Some(key) {
            self.hover_key = Some(key);
            self.hover_text = self.hover_text(cell);
        }
    }

    /// The HUD's state for a view (`camera`, `size`: where threat arrows point).
    fn fill_hud(&mut self, camera: Option<&Camera>, size: (u32, u32)) {
        if !self.hud.enabled() {
            return;
        }
        let s = &self.state;
        let content = Arc::clone(s.content_arc());
        let inspect = self.inspect.and_then(|cell| {
            if let Some(st) = s.stone(cell) {
                let def = content.stone(st.kind);
                Some(Inspect::Stone {
                    kind: st.kind.0,
                    tier: st.tier,
                    upgrade: def.tiers.get(usize::from(st.tier) + 1).map(|t| t.cost),
                    refund: st.invested * content.rules.refund_percent / 100,
                })
            } else {
                s.wall(cell).map(|w| Inspect::Wall { hp: w.hp, max_hp: w.max_hp, refund: (w.cost as i64 * i64::from(content.rules.refund_percent) / 100 * i64::from(w.hp) / i64::from(w.max_hp.max(1))) as i32 })
            }
        });
        let running = self.running();
        let next = s.next_wave_tick().filter(|_| running);
        let left = next.map(|t| t.saturating_sub(s.tick()));
        let info = HudInfo {
            content: &content,
            wave: s.wave(),
            waves: s.waves_total(),
            next_wave_s: left.map(|l| l.div_ceil(TICK_HZ)),
            early_bonus: left.map_or(0, |l| (i64::from(l) * i64::from(content.rules.early_call_milli_per_s) / (i64::from(TICK_HZ) * 1000)) as i32),
            dust: s.dust(),
            tombstones: s.tombstones(),
            paused: self.paused,
            speed: self.speed,
            selected: self.selected,
            hover: &self.hover_text,
            inspect,
            kills: s.wave_stats().iter().map(|w| w.killed).sum(),
            run_time_s: f64::from(s.tick()) / f64::from(TICK_HZ),
            outcome: self.outcome(),
            volume: self.volume,
            reduced_motion: self.reduced_motion,
            glyph_style: GLYPHS[self.glyphs].0,
        };
        hud::fill(&mut self.hud.state, &info, &mut self.hud_texts);
        hud::fill_scale(&mut self.hud.state, UI_SCALES[self.ui_scale]);
        if self.confirm == Some(Confirm::Restart) {
            self.hud.state.set_str("confirm_title", "Restart this run?").set_str("confirm_body", "The slab, your stones and the waves start over with the same seed.");
        } else if let Some((title, body)) = self.confirm.and_then(|c| self.confirm_text(c)) {
            self.hud.state.set_str("confirm_title", &title).set_str("confirm_body", &body);
        }
        self.hud.state.set_str("autosave_text", if self.autosave_on { "On" } else { "Off" });
        if self.hud.visible("saves") {
            self.fill_saves();
        }
        if self.confirm == Some(Confirm::Desync) {
            let (tick, what) = self.lex.as_ref().and_then(|l| l.desync.clone()).unwrap_or_default();
            let what: String = what.chars().take(160).collect();
            let body = format!("The Lex rules and the reference parted at tick {tick}: {what}. The run stopped there. Start it again?");
            self.hud.state.set_str("confirm_title", "The rules disagree").set_str("confirm_body", &body);
        }
        if let Some(cam) = camera {
            let groups = self.threat_groups();
            hud::fill_threats(&mut self.hud.state, cam, size, &groups, &mut self.hud_texts);
        }
        // The controls list only while it's open (prompts are made as text).
        if self.hud.visible("controls") {
            let bindings = self.hud.state.list_mut("bindings");
            bindings.clear();
            let input = self.controls.input();
            for a in input.map().actions().iter().filter(|a| !a.name.starts_with("ui_")).take(24) {
                let keys = input.prompt(&a.name).unwrap_or_default();
                let row = bindings.push();
                let label = if a.description.is_empty() { a.name.as_str() } else { a.description.as_str() };
                row.set_str("label", label).set_str("keys", &keys);
            }
        }
    }

    /// Where the dead are, by face: the centre of each group and how many.
    fn threat_groups(&self) -> [(&'static str, Vec3, usize); 5] {
        let mut acc: [(&'static str, Vec3, usize); 5] =
            [("North cliff", Vec3::ZERO, 0), ("East cliff", Vec3::ZERO, 0), ("West cliff", Vec3::ZERO, 0), ("South gorges", Vec3::ZERO, 0), ("Wraiths", Vec3::ZERO, 0)];
        let content = self.state.content();
        let half = content.map.half_z as f32 / 1000.0;
        for e in self.state.enemies() {
            let [x, y, z] = e.world_pos_f32();
            let p = Vec3::new(x, y, z);
            let i = match (e.face(content), e.motion) {
                (_, Motion::Air { .. }) => 4,
                (Some(Face::North), _) => 0,
                (Some(Face::East), _) => 1,
                (Some(Face::West), _) => 2,
                // On the top face: only the gorge mouths along the south rim count as a front.
                (Some(Face::Top), _) if z > half - 4.0 => 3,
                _ => continue,
            };
            acc[i].1 = acc[i].1 + p;
            acc[i].2 += 1;
        }
        acc.map(|(l, sum, n)| (l, if n > 0 { sum * (1.0 / n as f32) } else { sum }, n))
    }

    pub(crate) fn status_json(&self) -> Json {
        let mut j = self.status_core();
        j.set("rules", match &self.lex {
            Some(l) => l.status_json(),
            None if self.rules_lex => Json::obj([("rules", Json::from("lex")), ("off", Json::from("see the log: the Lex rules couldn't start"))]),
            None => Json::from("rust"),
        });
        j
    }

    fn status_core(&self) -> Json {
        let s = &self.state;
        Json::obj([
            ("seed", Json::from(s.seed().to_string())),
            ("wave", Json::from(u32::from(s.wave()))),
            ("waves_total", Json::from(u32::from(s.waves_total()))),
            ("dust", Json::from(s.dust())),
            ("tombstones", Json::from(s.tombstones())),
            ("outcome", Json::from(outcome_name(self.outcome()))),
            ("tick", Json::from(s.tick())),
            ("enemies", Json::from(s.enemies().len())),
            ("paused", Json::from(self.paused)),
            ("speed", Json::from(self.speed)),
            ("cutscene", self.cutscene.to_json()),
        ])
    }

    /// The HUD and the sound, for `play_state` (`include: ["ui"]` gives the widget tree).
    pub fn presentation_json(&self) -> Json {
        let content = self.state.content();
        let mut hud = Json::obj([
            ("on", Json::from(self.hud.enabled())),
            ("screens", Json::arr(self.hud.screens().into_iter().map(Json::from))),
            ("selected", Json::from(match self.selected.and_then(|i| content.stones.get(usize::from(i))) {
                Some(d) => d.id.as_str(),
                None => "wall",
            })),
            ("inspecting", self.inspect.map_or(Json::Null, |c| Json::from(content.map.cell_name(c)))),
        ]);
        if let Some(p) = &self.hud.problem {
            hud.set("problem", Json::from(p.as_str()));
        }
        Json::obj([("hud", hud), ("audio", self.sfx.to_json())])
    }

    /// Opens the panel of the stone or wall on `cell`, as clicking it does. False when there's
    /// nothing there to select.
    pub fn select_cell(&mut self, cell: CellId) -> bool {
        let there = self.state.stone(cell).is_some() || self.state.wall(cell).is_some();
        if there {
            self.inspect = Some(cell);
        }
        there
    }

    /// The HUD's widget tree (`play_ui`).
    pub fn ui_tree(&self, limit: usize) -> Json {
        self.hud.tree(limit)
    }

    /// Presses a HUD widget by id, as a click would (`play_ui`). Returns the action it sent.
    pub(crate) fn press(&mut self, world: &mut GameWorld<'_>, agent: &str, id: &str) -> Result<Option<String>, String> {
        let cam = self.camera;
        self.fill_hud(cam.as_ref(), (1280, 720));
        let action = self.hud.press(id)?;
        if let Some(a) = &action {
            self.ui_action(world, agent, a);
        }
        Ok(action)
    }

    /// Braziers' flames into `fire_buf`, for their crackle.
    fn fires(&mut self) {
        self.fire_buf.clear();
        let content = self.state.content();
        let Some(scene) = &self.scene else { return };
        let fires = self.state.stones().filter(|(_, s)| content.stone(s.kind).id == "brazier");
        self.fire_buf.extend(fires.map(|(c, _)| (c, scene.geo.anchor(&content.map, c).0 + Vec3::new(0.0, 1.8, 0.0))));
    }
}

pub fn outcome_name(o: Outcome) -> &'static str {
    match o {
        Outcome::Running => "running",
        Outcome::Won => "won",
        Outcome::Lost => "lost",
    }
}

/// STYLE.md lighting: a low cool moon, dim blue-violet ambient, ACES tone mapping, night sky.
pub fn night(world: &mut GameWorld<'_>) {
    // A low moon (about 30 degrees up) from the east-south-east: the east and south faces catch
    // it, the north and west fall into blue shadow, so turning the slab shows which face is which.
    let (elev, az) = (30f32.to_radians(), 60f32.to_radians());
    let moon = Vec3::new(elev.cos() * az.sin(), elev.sin(), elev.cos() * az.cos());
    let patch = EnvPatch {
        sun_direction: Some(moon),
        // A dim moon a shade less blue than the palette's swatch, and a blue fill that does most
        // of the lighting: the night is dark, but faces turned from the moon (the north and west
        // cliffs, where the dead climb) stay readable; the stone is a cool grey (STYLE.md:
        // neutrals under ~25% saturation), and the fire, the runes and the dead's eyes carry the
        // scene.
        sun_color: Some(palette::MOONLIGHT),
        sun_intensity: Some(0.55),
        ambient_color: Some(palette::NIGHT_FILL),
        ambient_intensity: Some(1.7),
        exposure: Some(1.0),
        tonemap: Some(true),
        background: Some(palette::SKY),
        // The moon casts one fitted shadow map (cheapest on the iGPU for this top-down view).
        shadow: Some(ShadowPatch { enabled: Some(true), distance: Some(120.0), softness: Some(1.5), ..Default::default() }),
        // Runes, eyes and fire glow; nothing else is bright enough to bloom.
        bloom: Some(BloomPatch { enabled: Some(true), threshold: Some(1.0), intensity: Some(0.6), ..Default::default() }),
        fxaa: Some(true),
        // A moonlit gradient with the moon's disc in the light's direction; below the horizon the
        // void stays near-black so the slab floats.
        sky: Some(SkyPatch { enabled: Some(true), ground: Some(palette::VOID), ..Default::default() }),
        // The slab floats over mist: thick below the cliffs, almost none on the top face. The
        // mist is the night's blue (STYLE.md MIST), a shade lighter than the shadowed cliffs, so
        // the slab stands out against it and the dead climbing out of it read as silhouettes.
        fog: Some(FogPatch {
            enabled: Some(true),
            color: Some(palette::MIST),
            density: Some(0.08),
            falloff: Some(0.35),
            height: Some(-14.0),
            sun_scatter: Some(0.15),
            ..Default::default()
        }),
        // The grade (STYLE.md): deeper, cooler shadows, highlights a touch warm (the fire's side),
        // stone pulled towards neutral by negative vibrance while the rune-cyan, grave-green and
        // fire families, already saturated, keep theirs; a soft vignette. No contrast boost: it
        // pushes the glowing runes into the tone curve's shoulder, where they wash out.
        grading: Some(GradingPatch {
            fields: GradingFields {
                enabled: Some(true),
                saturation: Some(0.8),
                vibrance: Some(-1.0),
                lift: Some([-0.01, -0.004, 0.012]),
                gain: Some([1.03, 1.0, 0.97]),
                vignette: Some(0.25),
                vignette_radius: Some(0.55),
                ..Default::default()
            },
            lut: None,
        }),
        // Contact shadows where stones, walls and the dead meet the slab, at half resolution
        // with 8 samples (the iGPU-safe setting).
        ssao: Some(SsaoPatch {
            enabled: Some(true),
            radius: Some(0.6),
            intensity: Some(1.0),
            samples: Some(8),
            resolution: Some(0.5),
        }),
        // A breeze for the grass (grass.rs): the far side of the slab thins out with distance.
        wind: Some(WindPatch {
            strength: Some(crate::grass::WIND.0),
            frequency: Some(crate::grass::WIND.1),
            gust: Some(crate::grass::WIND.2),
            fade_start: Some(crate::grass::FADE_START),
            fade_end: Some(crate::grass::FADE_END),
            shadow_end: Some(crate::grass::SHADOW_END),
            ..Default::default()
        }),
        physics: None,
    };
    if let Err(e) = world.set_env(&patch) {
        eprintln!("sepulcra: lighting: {e}");
    }
}

/// `--seed N` (a u64) and `--mute`, the arguments after `--`.
pub fn parse_args(args: &[String]) -> Result<(Option<u64>, bool), String> {
    let (mut seed, mut mute) = (None, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--seed" => {
                let v = it.next().ok_or("--seed needs a value (a u64)")?;
                seed = Some(v.trim().parse::<u64>().map_err(|_| format!("--seed must be a u64, got {v:?}"))?);
            }
            "--mute" => mute = true,
            other => return Err(format!("unknown argument '{other}' (sepulcra takes --seed N and --mute)")),
        }
    }
    Ok((seed, mute))
}

/// [`Game::state_hashes`] for a simulation state: `sim` (everything, [`State::hash`]) and its
/// parts `economy` (dust, tombstones, wave, outcome), `enemies`, `defences` (stones and walls) and
/// `projectiles`. Cheap: a few hundred words per tick.
pub(crate) fn state_hashes(state: &State, out: &mut Vec<(&'static str, u64)>) {
    use specula_core::hash::StableHasher;
    out.push(("sim", state.hash()));
    let mut h = StableHasher::with_seed(1);
    for v in [i64::from(state.dust()), i64::from(state.tombstones()), i64::from(state.wave()), state.outcome() as i64] {
        h.write_i64(v);
    }
    h.write_u64(state.next_wave_tick().map_or(u64::MAX, u64::from));
    out.push(("economy", h.finish()));
    let mut h = StableHasher::with_seed(2);
    for e in state.enemies() {
        h.write_u64(u64::from(e.id));
        h.write_u64(u64::from(e.kind.0));
        for v in [e.hp, e.pos.x, e.pos.y, e.pos.z, e.slow, e.remaining] {
            h.write_i64(i64::from(v));
        }
        h.write_bool(e.alive);
    }
    out.push(("enemies", h.finish()));
    let mut h = StableHasher::with_seed(3);
    for (cell, s) in state.stones() {
        h.write_u64(u64::from(cell.0));
        h.write_u64(u64::from(s.kind.0) << 8 | u64::from(s.tier));
        h.write_i64(i64::from(s.invested));
        h.write_u64(u64::from(s.ready_at));
    }
    for (cell, w) in state.walls() {
        h.write_u64(u64::from(cell.0) | 1 << 32);
        h.write_i64(i64::from(w.hp));
    }
    out.push(("defences", h.finish()));
    let mut h = StableHasher::with_seed(4);
    for p in state.projectiles() {
        h.write_u64(u64::from(p.id));
        for v in [p.pos.x, p.pos.y, p.pos.z] {
            h.write_i64(i64::from(v));
        }
    }
    out.push(("projectiles", h.finish()));
}

impl Game for SepulcraGame {
    fn name(&self) -> &str {
        "sepulcra"
    }

    fn tick_hz(&self) -> u32 {
        TICK_HZ
    }

    fn configure(&mut self, args: &[String]) -> Result<(), String> {
        // `--rules lex|rust` (see crate::lexmode); the rest are parse_args's.
        let mut rest = Vec::new();
        let mut it = args.iter();
        while let Some(a) = it.next() {
            if a == "--intro" {
                match it.next().map(String::as_str) {
                    Some("on") => self.cutscene.intro_off = false,
                    Some("off") => self.cutscene.intro_off = true,
                    other => return Err(format!("--intro takes on or off, got {other:?}")),
                }
            } else if a == "--rules" {
                match it.next().map(String::as_str) {
                    Some("lex") => self.rules_lex = true,
                    Some("rust") => self.rules_lex = false,
                    other => return Err(format!("--rules takes lex or rust, got {other:?}")),
                }
            } else {
                rest.push(a.clone());
            }
        }
        let (seed, mute) = parse_args(&rest)?;
        if let Some(seed) = seed {
            self.seed_arg = Some(seed);
            self.state = State::new(self.content(), seed);
        }
        self.mute = mute;
        if self.rules_lex {
            self.lex = Some(crate::lexmode::LexRules::new(self.state.content(), self.state.seed())?);
        }
        Ok(())
    }

    fn modal_changed(&mut self, ctx: &mut Ctx, open: bool) {
        // The engine's review or gallery screen holds the window: the run waits (the same active
        // pause as P, journaled like any frame), and resumes when it closes unless the player had
        // paused it already.
        if open && self.running() && !self.paused {
            self.apply_pause(&mut ctx.world, true);
            self.modal_paused = true;
        } else if !open && self.modal_paused {
            self.modal_paused = false;
            self.apply_pause(&mut ctx.world, false);
        }
    }

    fn assets_changed(&mut self, _ctx: &mut Ctx, paths: &[String]) {
        // Screens, the theme and sounds come from the assets folder: reload what changed (the
        // engine reloads recipes itself).
        let stem = |p: &str, dir: &str| p.strip_prefix(dir).and_then(|r| r.strip_suffix(".json")).filter(|r| !r.contains('/')).map(str::to_owned);
        let screens: Vec<String> = paths.iter().filter_map(|p| stem(p, "ui/")).collect();
        let sounds: Vec<String> = paths.iter().filter_map(|p| stem(p, "sounds/")).collect();
        let assets = self.assets.clone().unwrap_or_else(SepulcraGame::default_assets);
        if !screens.is_empty() {
            for e in self.hud.reload(&screens) {
                eprintln!("sepulcra: hot reload: {e}");
            }
        }
        let pieces: Vec<String> = paths.iter().filter_map(|p| stem(p, "music/")).collect();
        if !sounds.is_empty() || !pieces.is_empty() {
            self.sfx.reload(assets, &sounds, &pieces);
        }
    }

    fn lex_natives(&self) -> specula_lex::Natives {
        sepulcra::lex::natives()
    }

    fn assets_root(&self) -> Option<PathBuf> {
        Some(self.assets.clone().unwrap_or_else(SepulcraGame::default_assets))
    }

    fn use_assets(&mut self, root: &std::path::Path) {
        self.assets = Some(root.to_path_buf());
    }

    fn use_audio(&mut self, audio: Arc<specula::audio_service::AudioService>) {
        self.engine_audio = Some(audio);
    }

    fn start(&mut self, ctx: &mut Ctx) {
        night(&mut ctx.world);
        // Screens and sounds from the assets folder (agents edit them there), else the built-in
        // copies.
        let assets = self.assets.clone().unwrap_or_else(SepulcraGame::default_assets);
        let ui_dir = assets.join("ui");
        self.hud = Hud::new(Some(&ui_dir), crate::controls::bindings().expect("the built-in bindings are valid (tested)"));
        // Autosaves are for people playing: on with a window, off headless (agents' runs).
        self.autosave_on = ctx.windowed;
        // Sound and music through the engine's audio: off headless unless the engine was started
        // with `--audio null` (then agents hear a headless run too), never with `--mute`.
        self.sfx = match (&self.engine_audio, self.mute) {
            (_, true) => Sfx::off("muted (--mute)"),
            (None, false) => Sfx::off("no engine audio"),
            (Some(a), false) => Sfx::start(Arc::clone(a), Some(assets.clone())),
        };
        self.sfx.set_volume(self.volume);
        self.rebuild(&mut ctx.world, ctx.agent);
        self.say(format!("Seed {}: carve stones before wave 1", self.state.seed()));
        // The intro flyover (windowed, unless --intro off or reduced motion): wave 1 waits for it.
        self.cutscene.intro(ctx.windowed, self.reduced_motion);
    }

    fn tick(&mut self, ctx: &mut Ctx) {
        // Rendered sounds and music land here too: headless engines (`--audio null`) have no frames.
        self.sfx.poll();
        self.events.clear();
        self.events.append(&mut self.pending);
        // Events before this come from commands (their autosave was already asked for).
        let from_steps = self.events.len();
        let mut stepped = 0;
        let was = self.lex_flags();
        // A cutscene holding the run (the intro) waits like a pause: no step, no build time lost.
        if !self.paused && !self.cutscene.holds() {
            for _ in 0..self.speed {
                if !self.running() {
                    break;
                }
                if let Some((_, player)) = &mut self.autoplay {
                    let tick = self.state.tick();
                    for c in player.drive(&mut self.state, &mut self.events) {
                        if let Some(lex) = &mut self.lex {
                            lex.follow(&self.state, &c);
                        }
                        self.journal.push((tick, c));
                    }
                }
                self.state.step_into(&[], &mut self.events);
                if let Some(lex) = &mut self.lex {
                    lex.step(&self.state);
                }
                stepped += 1;
            }
        }
        self.lex_verdicts(was);
        let events = std::mem::take(&mut self.events);
        self.messages(&events);
        if events.iter().any(|e| matches!(e, Event::Won)) {
            self.cutscene.victory(ctx.windowed);
        }
        // The rotating autosave as a wave starts (the save holds this tick, wave and all).
        if let Some(wave) = wave_started(&events[from_steps..]) {
            self.autosave(wave);
        }
        // Flashes keep fading after the run ends (no more steps, but time still passes).
        let advance = if self.paused { 0 } else { u64::from(self.speed).max(stepped) };
        self.pres_ticks += advance;
        self.now = self.pres_ticks as f32 / TICK_HZ as f32;
        self.sounds(&events);
        // Window frames after this tick draw presentation time from (pres_ticks - advance) at
        // alpha 0 up to pres_ticks at alpha 1; flashes born in this tick start at the beginning
        // of that span, so the first frame after the tick already shows them.
        let born = (self.pres_ticks - advance) as f32 / TICK_HZ as f32;
        let framed = self.last_frame.is_some_and(|t| t.elapsed() < FRAMED_WITHIN);
        if let Some(scene) = &mut self.scene {
            scene.sync(&mut ctx.world, &self.state, &events, born);
            // Without window frames (headless), or while agents step a paused clock (a capture
            // may come before the next frame), put everything where it is at this tick.
            // Otherwise the next window frame moves things anyway: don't write them twice.
            if !framed || ctx.clock.paused() {
                scene.animate(&mut ctx.world, 1.0, self.now);
            }
        }
        self.events = events;
    }

    fn frame(&mut self, ctx: &mut Ctx, frame: &mut Frame) {
        self.last_frame = Some(Instant::now());
        self.sfx.poll();
        // Frames draw the span between the last two ticks: positions at prev..cur by alpha, and
        // presentation time to match. When either the game or the engine clock is paused there is
        // no span: draw the newest tick (alpha 1), as play_state reports it.
        let still = self.paused || ctx.clock.paused();
        let alpha = if still { 1.0 } else { frame.alpha };
        let sub = (alpha - 1.0) * self.speed as f32;
        self.now = ((self.pres_ticks as f32) + sub).max(0.0) / TICK_HZ as f32;
        if self.scene.is_none() {
            return;
        }
        // Input reaches the HUD first; the game gets what the HUD didn't take. The HUD shows the
        // state as of now (threat arrows from last frame's camera).
        // The input that skipped a cutscene goes nowhere else (a click doesn't also build).
        if self.cutscene.take_swallow() {
            self.controls.begin(&[], &[]);
        } else {
            self.controls.begin(frame.input, frame.pads);
        }
        let cam = self.camera;
        self.fill_hud(cam.as_ref(), frame.size);
        let mut ui_events = std::mem::take(&mut self.ui_frame);
        self.hud.frame(self.controls.input(), frame.size, &mut ui_events);
        self.ui_events(&mut ctx.world, ctx.agent, &ui_events);
        let map = Arc::clone(self.state.content_arc());
        let Some(scene) = &self.scene else { return };
        let actions = self.controls.act(ui_events.consumed, frame.dt_s.min(0.25), frame.size, &scene.geo, &map.map);
        self.ui_frame = ui_events;
        for a in actions {
            self.act(&mut ctx.world, ctx.agent, a);
        }
        self.controls.sync_camera(frame.camera, frame.agent_view);
        self.camera = *frame.camera;
        self.hover(&mut ctx.world);
        if let Some(scene) = &mut self.scene {
            scene.animate(&mut ctx.world, alpha, self.now);
        }
        if let Some(cam) = &self.camera {
            self.sfx.listen(cam);
        }
        if self.sfx.enabled() {
            self.fires();
            self.sfx.fires(&self.fire_buf);
        }
        if !self.title_sent {
            *frame.title = Some("Sepulcra".into());
            self.title_sent = true;
        }
    }

    fn overlay(&mut self, _ctx: &mut Ctx, overlay: &mut Overlay) {
        if overlay.capture {
            // A capture's own camera and size (threat arrows point from its view).
            let cam = overlay.camera;
            self.fill_hud(Some(&cam), overlay.size);
        }
        self.hud.overlay(overlay, self.now);
    }

    fn before_capture(&mut self, ctx: &mut Ctx) {
        if let Some(scene) = &mut self.scene {
            scene.animate(&mut ctx.world, 1.0, self.now);
        }
    }

    fn tools(&self) -> Vec<ToolDef> {
        tools::defs()
    }

    /// play_record writes a recording file (a playtest counts it toward its file cap).
    fn file_tools(&self) -> &[&str] {
        &["play_record"]
    }

    fn call_tool(&mut self, ctx: &mut Ctx, name: &str, args: &Json) -> ToolOutput {
        // An agent acting on the run is never held by a cutscene.
        if matches!(name, "play_new" | "play_command" | "play_control" | "play_ui" | "play_replay") {
            self.cutscene.agent_acted();
        }
        tools::call(self, ctx, name, args)
    }

    fn timeline_requests(&mut self, out: &mut Vec<specula::game::TimelineRequest>) {
        self.cutscene.take_requests(out);
    }

    fn timeline_event(&mut self, _ctx: &mut Ctx, event: &specula::game::TimelineEvent) {
        if let Some(line) = self.cutscene.event(event) {
            self.say(line);
        }
        if let specula::game::TimelineWhat::Cue(cue) = &event.what {
            if let Some(name) = crate::cutscene::cue_sound(&cue.sound) {
                self.sfx.ui(name, self.now);
            }
        }
    }

    fn status(&self) -> Json {
        self.status_json()
    }

    /// The desync detector's view of the simulation (it lives outside the world): the whole
    /// state's hash, and its parts, so a divergence names what diverged first.
    fn state_hashes(&self, out: &mut Vec<(&'static str, u64)>) {
        state_hashes(&self.state, out);
        if let Some(l) = &self.lex {
            out.push(("lex", l.sim.hash()));
        }
    }

    fn save_state(&self) -> Option<Result<Vec<u8>, String>> {
        Some(self.save_bytes())
    }

    fn parse_state(&self, data: &[u8]) -> Result<Box<dyn Any>, String> {
        self.parse_saved(data).map(|l| Box::new(l) as Box<dyn Any>)
    }

    fn state_parser(&self) -> Option<specula::game::StateParser> {
        // Loads parse the game's section on their worker, against the content this game runs.
        let content = self.content();
        Some(Box::new(move |data: &[u8]| saves::parse_saved(&content, data).map(|l| Box::new(l) as Box<dyn Any + Send>)))
    }

    fn apply_state(&mut self, ctx: &mut Ctx, state: Box<dyn Any>) {
        if let Ok(l) = state.downcast::<saves::Loaded>() {
            self.apply_saved(&mut ctx.world, ctx.agent, *l);
        }
    }

    fn take_requests(&mut self, out: &mut Vec<specula::game::GameRequest>) {
        out.append(&mut self.requests);
    }

    fn request_done(&mut self, ctx: &mut Ctx, request: &specula::game::GameRequest, result: Result<Json, Json>) {
        self.request_result(&mut ctx.world, request, result);
    }

    fn checkpoint(&self) -> Option<Box<dyn Any>> {
        Some(Box::new(Checkpoint {
            state: self.state.clone(),
            journal: self.journal.clone(),
            autoplay: self.autoplay.clone(),
            scene: self.scene.clone(),
            selected: self.selected,
            inspect: self.inspect,
            paused: self.paused,
            speed: self.speed,
            events: self.events.clone(),
            pending: self.pending.clone(),
            now: self.now,
            pres_ticks: self.pres_ticks,
            log: self.log.clone(),
            screens: self.hud.screens(),
            confirm: self.confirm,
            radial_cell: self.radial_cell,
            lex: self.lex.clone(),
        }))
    }

    fn restore(&mut self, checkpoint: Box<dyn Any>) {
        let Ok(c) = checkpoint.downcast::<Checkpoint>() else { return };
        let c = *c;
        self.state = c.state;
        self.journal = c.journal;
        self.autoplay = c.autoplay;
        self.scene = c.scene;
        self.selected = c.selected;
        self.inspect = c.inspect;
        self.paused = c.paused;
        self.modal_paused = false;
        self.speed = c.speed;
        self.events = c.events;
        self.pending = c.pending;
        self.now = c.now;
        self.pres_ticks = c.pres_ticks;
        self.log = c.log;
        self.confirm = c.confirm;
        self.radial_cell = c.radial_cell;
        self.lex = c.lex;
        for screen in ["pause", "victory", "defeat", "radial", "confirm", "settings", "controls", "saves"] {
            let open = c.screens.iter().any(|s| s == screen);
            if self.hud.visible(screen) != open {
                self.hud.show(screen, open);
            }
        }
        // Presentation caches are rebuilt on the next frame.
        self.hover_key = None;
    }
}

/// The last wave started among `events`, if any.
fn wave_started(events: &[Event]) -> Option<u16> {
    events.iter().rev().find_map(|e| if let Event::WaveStarted { wave, .. } = e { Some(*wave) } else { None })
}

#[cfg(test)]
mod desync_tests {
    use super::*;
    use std::sync::Arc;

    fn hashes(state: &State) -> Vec<(&'static str, u64)> {
        let mut out = Vec::new();
        state_hashes(state, &mut out);
        out
    }

    /// The desync detector's subsystem hashes: equal for equal runs; the seed is in `sim`.
    #[test]
    fn subsystem_hashes_follow_the_simulation() {
        let content = Arc::new(Content::builtin());
        let (mut a, mut b) = (State::new(Arc::clone(&content), 7), State::new(Arc::clone(&content), 7));
        for _ in 0..200 {
            a.step(&[]);
            b.step(&[]);
        }
        assert_eq!(hashes(&a), hashes(&b));
        let names: Vec<&str> = hashes(&a).iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["sim", "economy", "enemies", "defences", "projectiles"]);
        let (seven, eight) = (State::new(Arc::clone(&content), 7), State::new(content, 8));
        assert_ne!(hashes(&seven)[0].1, hashes(&eight)[0].1, "the seed is in sim");
    }
}

/// Review round 1 (finding 6): with `--rules lex`, the Lex run decides the outcome, and a
/// disagreement with the reference stops the game visibly.
#[cfg(test)]
mod lex_rules_tests {
    use super::*;
    use specula::game::Clock;
    use specula::world::World;

    fn lex_game(seed: u64) -> (SepulcraGame, World, Clock) {
        let mut g = SepulcraGame::new(Arc::new(Content::builtin()), seed);
        let mut world = World::default();
        let clock = Clock::new(g.tick_hz());
        g.start(&mut Ctx { world: GameWorld::new(&mut world), clock: &clock, agent: "sepulcra", windowed: false });
        g.rules_lex = true;
        g.start_lex();
        assert!(g.lex.is_some());
        (g, world, clock)
    }

    fn tick(g: &mut SepulcraGame, world: &mut World, clock: &mut Clock) {
        g.tick(&mut Ctx { world: GameWorld::new(world), clock, agent: "sepulcra", windowed: false });
        clock.finish_tick();
    }

    #[test]
    fn a_desync_stops_the_game_with_a_dialog() {
        let (mut g, mut world, mut clock) = lex_game(3);
        for _ in 0..5 {
            tick(&mut g, &mut world, &mut clock);
        }
        assert!(g.running());
        // The Lex run takes a stone the reference never saw: the next comparison differs.
        let content = g.state.content().clone();
        let cmd = Command::PlaceStone { cell: content.map.cell_ref("top:8,12").unwrap(), stone: content.stone_kind("warding").unwrap() };
        let lex = g.lex.as_mut().unwrap();
        let call = sepulcra::lex::command_call(&lex.sim, &content, &cmd).unwrap();
        lex.sim.apply(&call).unwrap();
        tick(&mut g, &mut world, &mut clock);
        let at = g.state.tick();
        assert!(!g.running(), "stopped");
        assert_eq!(g.confirm, Some(Confirm::Desync));
        assert!(g.hud.visible("confirm"), "the dialog is up");
        for _ in 0..10 {
            tick(&mut g, &mut world, &mut clock);
        }
        assert_eq!(g.state.tick(), at, "neither sim steps after a desync");
        let rules = g.status_json().get("rules").cloned().unwrap();
        assert_eq!(rules.get("parity").and_then(Json::as_str), Some("desync"), "{rules}");
        // Commands are refused, with the reason as data.
        let r = g.command(&mut GameWorld::new(&mut world), Command::CallWave);
        assert_eq!(r, Err(Rejection::GameOver));
        assert_eq!(g.lex.as_ref().unwrap().last_refusal.as_ref().map(|r| r.0.as_str()), Some("desync"));
    }

    #[test]
    fn lex_judges_commands_first_and_nothing_drifts() {
        let (mut g, mut world, _clock) = lex_game(4);
        let content = g.state.content().clone();
        let cell = content.map.cell_ref("top:8,12").unwrap();
        let warding = content.stone_kind("warding").unwrap();
        // Both refuse: the game's rejection with Lex's own code beside it.
        let lane = Command::PlaceStone { cell: content.map.cell_ref("top:9,14").unwrap(), stone: warding };
        let r = g.command(&mut GameWorld::new(&mut world), lane).unwrap_err();
        assert_eq!(r, Rejection::NotBuildable);
        assert!(g.lex.as_ref().unwrap().last_refusal.is_some());
        assert!(g.running(), "agreeing refusals are no desync");
        // The reference has the stone, Lex doesn't: Lex would accept, the reference refuses ...
        let mut pending = Vec::new();
        g.state.apply(&Command::PlaceStone { cell, stone: warding }, &mut pending).unwrap();
        let hash = g.lex.as_ref().unwrap().sim.hash();
        let r = g.command(&mut GameWorld::new(&mut world), Command::PlaceStone { cell, stone: warding });
        assert_eq!(r, Err(Rejection::Occupied));
        assert_eq!(g.lex.as_ref().unwrap().sim.hash(), hash, "... and the Lex run takes it back");
        assert!(!g.running(), "a desync");
        assert_eq!(g.confirm, Some(Confirm::Desync), "shown at once");

        // The other way round: Lex refuses with its code mapped, not game_over.
        let (mut g, mut world, _clock) = lex_game(4);
        let lex = g.lex.as_mut().unwrap();
        let call = sepulcra::lex::command_call(&lex.sim, &content, &Command::PlaceStone { cell, stone: warding }).unwrap();
        lex.sim.apply(&call).unwrap();
        let r = g.command(&mut GameWorld::new(&mut world), Command::PlaceStone { cell, stone: warding });
        assert_eq!(r, Err(Rejection::Occupied));
        assert!(g.state.stone(cell).is_none(), "the reference didn't take it either");
        assert!(!g.running());
    }

    /// Save games in `--rules lex` mode: the game section carries the Lex run's state, so a
    /// load resumes it exactly (same Lex hash, parity on); an older section without it replays
    /// the recording through Lex to the same state.
    #[test]
    fn a_lex_mode_game_saves_and_loads_with_its_lex_state() {
        let (mut g, mut world, mut clock) = lex_game(6);
        let content = g.state.content().clone();
        let warding = content.stone_kind("warding").unwrap();
        for c in ["top:8,12", "top:12,8"] {
            g.command(&mut GameWorld::new(&mut world), Command::PlaceStone { cell: content.map.cell_ref(c).unwrap(), stone: warding }).unwrap();
        }
        for _ in 0..1_200 {
            tick(&mut g, &mut world, &mut clock);
        }
        assert!(g.running() && g.state.enemies().len() + g.state.projectiles().len() > 0, "mid-wave");
        let bytes = g.save_bytes().unwrap();
        let resume = |bytes: &[u8]| {
            let (mut h, mut world2, clock2) = lex_game(6);
            let loaded = h.parse_saved(bytes).unwrap();
            h.apply_saved(&mut GameWorld::new(&mut world2), "sepulcra", loaded);
            (h, world2, clock2)
        };
        let (mut h, mut world2, mut clock2) = resume(&bytes);
        let lex = |x: &SepulcraGame| x.lex.as_ref().map(|l| (l.sim.hash(), l.sim.tick(), l.desync.clone()));
        assert_eq!(lex(&h), lex(&g), "the Lex run resumes as saved");
        // An older section (version 1: no Lex part) replays the recording through Lex.
        let mut v1 = {
            let l = g.lex.take();
            let b = g.save_bytes().unwrap();
            g.lex = l;
            b
        };
        v1.truncate(v1.len() - 1);
        v1[..4].copy_from_slice(&1u32.to_le_bytes());
        let (old, _, _) = resume(&v1);
        assert_eq!(lex(&old), lex(&g), "replayed to the same Lex state");
        for _ in 0..600 {
            tick(&mut g, &mut world, &mut clock);
            tick(&mut h, &mut world2, &mut clock2);
        }
        assert_eq!(lex(&h), lex(&g), "and steps on identically");
        assert_eq!(h.state.tick(), g.state.tick());
        assert_eq!(h.lex.as_ref().unwrap().desync, None, "parity holds after the load");
    }

    /// Review round 6 (desyncui.py): loading a save whose Lex run had parted from the reference
    /// stops the game with the dialog up, as the desync itself did.
    #[test]
    fn a_loaded_desynced_lex_run_shows_its_dialog() {
        let (mut g, mut world, mut clock) = lex_game(3);
        tick(&mut g, &mut world, &mut clock);
        let content = g.state.content().clone();
        let cmd = Command::PlaceStone { cell: content.map.cell_ref("top:8,12").unwrap(), stone: content.stone_kind("warding").unwrap() };
        let lex = g.lex.as_mut().unwrap();
        let call = sepulcra::lex::command_call(&lex.sim, &content, &cmd).unwrap();
        lex.sim.apply(&call).unwrap();
        tick(&mut g, &mut world, &mut clock);
        assert!(g.lex.as_ref().unwrap().desync.is_some());
        let bytes = g.save_bytes().unwrap();
        let (mut h, mut world2, _clock2) = lex_game(3);
        let loaded = h.parse_saved(&bytes).unwrap();
        h.apply_saved(&mut GameWorld::new(&mut world2), "sepulcra", loaded);
        assert!(!h.running(), "stopped");
        assert_eq!(h.confirm, Some(Confirm::Desync));
        assert!(h.hud.visible("confirm"), "the dialog is up after the load");
    }

    #[test]
    fn the_end_screens_follow_the_lex_outcome() {
        let (mut g, _world, _clock) = lex_game(5);
        // The reference saying "won" while Lex runs on shows nothing (the comparison catches it).
        g.events.push(Event::Won);
        g.lex_verdicts((false, false));
        assert!(!g.events.iter().any(|e| matches!(e, Event::Won)));
        assert!(g.running());
        assert_eq!(g.outcome(), Outcome::Running);
    }
}

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
use specula::effects::{BloomPatch, FogPatch, GradingFields, GradingPatch, ShadowPatch, SkyPatch, SsaoPatch};
use specula::game::{Ctx, Frame, Game, Overlay};
use specula::sight::Camera;
use specula::world::{EnvPatch, World};
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

/// A window frame this recent means another follows the next tick.
const FRAMED_WITHIN: Duration = Duration::from_millis(250);
const LOG_LEN: usize = 32;
pub const SPEEDS: [u32; 3] = [1, 2, 4];
/// Button-prompt styles the settings cycle through (`Auto` follows the pad).
const GLYPHS: [(&str, Option<PadStyle>); 4] =
    [("Auto", None), ("Xbox", Some(PadStyle::Xbox)), ("PlayStation", Some(PadStyle::PlayStation)), ("Steam Deck", Some(PadStyle::SteamDeck))];

/// What the open confirmation dialog will do on "yes".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Confirm {
    Restart,
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
    /// `--mute`: no audio device.
    mute: bool,
    /// Where the game's assets are (recipes, anim sets, sounds, screens).
    assets: Option<PathBuf>,
    /// Settings (the settings screen).
    ui_scale: usize,
    volume: f32,
    reduced_motion: bool,
    glyphs: usize,
    /// The window camera last frame (threat arrows, listener).
    camera: Option<Camera>,
    /// The title last sent.
    title_sent: bool,
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
            assets: None,
            ui_scale: 2,
            volume: 0.8,
            reduced_motion: false,
            glyphs: 0,
            camera: None,
            title_sent: false,
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
    pub fn command(&mut self, world: &mut World, cmd: Command) -> Result<(), Rejection> {
        let tick = self.state.tick();
        self.state.apply(&cmd, &mut self.pending)?;
        self.journal.push((tick, cmd));
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
    pub fn restart(&mut self, world: &mut World, agent: &str, content: Arc<Content>, seed: u64, autoplay: Option<(String, Player)>) {
        self.state = State::new(content, seed);
        self.journal.clear();
        self.autoplay = autoplay;
        self.events.clear();
        self.pending.clear();
        self.log.clear();
        self.paused = false;
        self.inspect = None;
        self.sfx.reset();
        for screen in ["pause", "victory", "defeat", "radial", "confirm"] {
            self.hud.show(screen, false);
        }
        self.rebuild(world, agent);
        self.say(format!("New game, seed {seed}"));
    }

    /// Replaces the state (a loaded replay) and rebuilds the scene.
    pub(crate) fn load(&mut self, world: &mut World, agent: &str, state: State, journal: Journal) {
        self.state = state;
        self.journal = journal;
        self.autoplay = None;
        self.events.clear();
        self.pending.clear();
        self.inspect = None;
        self.rebuild(world, agent);
    }

    fn rebuild(&mut self, world: &mut World, agent: &str) {
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
    pub(crate) fn set_paused(&mut self, world: &mut World, paused: bool) {
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
    pub(crate) fn open_menu(&mut self, world: &mut World) {
        if self.state.outcome() == Outcome::Running {
            self.set_paused(world, true);
            self.hud.show("pause", true);
        }
    }

    pub(crate) fn set_speed(&mut self, world: &mut World, speed: u32) {
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
            Rejection::NotWallable => "walls go on top-face lanes".into(),
            Rejection::EnemyInTheWay { .. } => "an enemy is in the way".into(),
            Rejection::MaxTier => "already tier III".into(),
            r => r.code().replace('_', " "),
        };
        format!("Can't {what}: {hint}")
    }

    /// Plays a command from the player (mouse, pad, touch, HUD): refusals are said and buzzed.
    fn try_command(&mut self, world: &mut World, cmd: Command) -> bool {
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

    fn act(&mut self, world: &mut World, agent: &str, action: Action) {
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
                if self.state.outcome() != Outcome::Running {
                    self.new_run(world, agent);
                } else {
                    self.say("The run is still going (N starts a new game after the end)".into());
                }
                return;
            }
        };
        self.try_command(world, cmd);
    }

    fn new_run(&mut self, world: &mut World, agent: &str) {
        let seed = self.state.seed().wrapping_add(1);
        let content = self.content();
        self.restart(world, agent, content, seed, None);
    }

    /// What the HUD's buttons and menus asked for.
    fn ui_events(&mut self, world: &mut World, agent: &str, events: &UiEvents) {
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
    pub(crate) fn ui_action(&mut self, world: &mut World, agent: &str, action: &str) {
        let content = self.content();
        let stone_of = |icon: &str| content.stones.iter().position(|s| hud::icon_of(&s.id) == icon).map(|i| i as u8);
        match action {
            "call_wave" => {
                self.try_command(world, Command::CallWave);
            }
            "pause" => self.set_paused(world, true),
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
                self.glyphs = (self.glyphs + 1) % GLYPHS.len();
                if let Some(style) = GLYPHS[self.glyphs].1 {
                    self.controls.input_mut().set_pad_style(style);
                }
            }
            "toggle_motion" => self.reduced_motion = !self.reduced_motion,
            "restart" => {
                self.confirm = Some(Confirm::Restart);
                self.hud.show("confirm", true);
            }
            "confirm_yes" => {
                self.hud.show("confirm", false);
                if self.confirm.take() == Some(Confirm::Restart) {
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
        }
        t
    }

    /// Cursor and range ring for the hovered (or selected) cell.
    fn hover(&mut self, world: &mut World) {
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
        let next = s.next_wave_tick().filter(|_| s.outcome() == Outcome::Running);
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
            outcome: s.outcome(),
            volume: self.volume,
            reduced_motion: self.reduced_motion,
            glyph_style: GLYPHS[self.glyphs].0,
        };
        hud::fill(&mut self.hud.state, &info);
        hud::fill_scale(&mut self.hud.state, UI_SCALES[self.ui_scale]);
        if self.confirm == Some(Confirm::Restart) {
            self.hud.state.set_str("confirm_title", "Restart this run?").set_str("confirm_body", "The slab, your stones and the waves start over with the same seed.");
        }
        if let Some(cam) = camera {
            let groups = self.threat_groups();
            let refs: Vec<(&str, Vec3, usize)> = groups.iter().map(|(l, p, n)| (*l, *p, *n)).collect();
            hud::fill_threats(&mut self.hud.state, cam, size, &refs);
        }
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

    /// Where the dead are, by face: the centre of each group and how many.
    fn threat_groups(&self) -> Vec<(&'static str, Vec3, usize)> {
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
        acc.iter().filter(|g| g.2 > 0).map(|&(l, sum, n)| (l, sum * (1.0 / n as f32), n)).collect()
    }

    pub(crate) fn status_json(&self) -> Json {
        let s = &self.state;
        Json::obj([
            ("seed", Json::from(s.seed().to_string())),
            ("wave", Json::from(u32::from(s.wave()))),
            ("waves_total", Json::from(u32::from(s.waves_total()))),
            ("dust", Json::from(s.dust())),
            ("tombstones", Json::from(s.tombstones())),
            ("outcome", Json::from(outcome_name(s.outcome()))),
            ("tick", Json::from(s.tick())),
            ("enemies", Json::from(s.enemies().len())),
            ("paused", Json::from(self.paused)),
            ("speed", Json::from(self.speed)),
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
    pub(crate) fn press(&mut self, world: &mut World, agent: &str, id: &str) -> Result<Option<String>, String> {
        let cam = self.camera;
        self.fill_hud(cam.as_ref(), (1280, 720));
        let action = self.hud.press(id)?;
        if let Some(a) = &action {
            self.ui_action(world, agent, a);
        }
        Ok(action)
    }

    /// Braziers' flames, for their crackle.
    fn fires(&self) -> Vec<(CellId, Vec3)> {
        let content = self.state.content();
        let Some(scene) = &self.scene else { return Vec::new() };
        self.state
            .stones()
            .filter(|(_, s)| content.stone(s.kind).id == "brazier")
            .map(|(c, _)| (c, scene.geo.anchor(&content.map, c).0 + Vec3::new(0.0, 1.8, 0.0)))
            .collect()
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
pub fn night(world: &mut World) {
    // A low moon (about 30 degrees up) from the east-south-east: the east and south faces catch
    // it, the north and west fall into blue shadow, so turning the slab shows which face is which.
    let (elev, az) = (30f32.to_radians(), 60f32.to_radians());
    let moon = Vec3::new(elev.cos() * az.sin(), elev.sin(), elev.cos() * az.cos());
    let patch = EnvPatch {
        sun_direction: Some(moon),
        // A moon a shade less blue than the palette's swatch, and a near-neutral fill: the
        // slab's stone stays a cool grey (STYLE.md: neutrals under ~25% saturation) instead of
        // turning the whole night blue, so the cyan, green and fire families carry the colour.
        sun_color: Some(palette::MOONLIGHT),
        sun_intensity: Some(1.5),
        ambient_color: Some(palette::NIGHT_FILL),
        ambient_intensity: Some(2.7),
        exposure: Some(1.1),
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
        // The slab floats over mist: thick below the cliffs, almost none on the top face. The mist
        // is darker than the moonlit stone so the slab's silhouette stays crisp.
        fog: Some(FogPatch {
            enabled: Some(true),
            color: Some(palette::DEEP_MIST),
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

impl Game for SepulcraGame {
    fn name(&self) -> &str {
        "sepulcra"
    }

    fn tick_hz(&self) -> u32 {
        TICK_HZ
    }

    fn configure(&mut self, args: &[String]) -> Result<(), String> {
        let (seed, mute) = parse_args(args)?;
        if let Some(seed) = seed {
            self.seed_arg = Some(seed);
            self.state = State::new(self.content(), seed);
        }
        self.mute = mute;
        Ok(())
    }

    fn assets_root(&self) -> Option<PathBuf> {
        Some(self.assets.clone().unwrap_or_else(SepulcraGame::default_assets))
    }

    fn start(&mut self, ctx: &mut Ctx) {
        night(ctx.world);
        // The screens from the assets folder (agents edit them there), else the built-in copies.
        let ui_dir = self.assets.clone().unwrap_or_else(SepulcraGame::default_assets).join("ui");
        self.hud = Hud::new(Some(&ui_dir), crate::controls::bindings().expect("the built-in bindings are valid (tested)"));
        if ctx.windowed {
            self.controls.enable_gamepads();
            if !self.mute {
                self.sfx = Sfx::start();
                self.sfx.set_volume(self.volume);
            } else {
                self.sfx = Sfx::off("muted (--mute)");
            }
        } else {
            self.sfx = Sfx::off("headless");
        }
        self.rebuild(ctx.world, ctx.agent);
        self.say(format!("Seed {}: carve stones before wave 1", self.state.seed()));
    }

    fn tick(&mut self, ctx: &mut Ctx) {
        self.events.clear();
        self.events.append(&mut self.pending);
        let mut stepped = 0;
        if !self.paused {
            for _ in 0..self.speed {
                if self.state.outcome() != Outcome::Running {
                    break;
                }
                if let Some((_, player)) = &mut self.autoplay {
                    let tick = self.state.tick();
                    for c in player.drive(&mut self.state, &mut self.events) {
                        self.journal.push((tick, c));
                    }
                }
                self.state.step_into(&[], &mut self.events);
                stepped += 1;
            }
        }
        let events = std::mem::take(&mut self.events);
        self.messages(&events);
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
            scene.sync(ctx.world, &self.state, &events, born);
            // Without window frames (headless), or while agents step a paused clock (a capture
            // may come before the next frame), put everything where it is at this tick.
            // Otherwise the next window frame moves things anyway: don't write them twice.
            if !framed || ctx.clock.paused() {
                scene.animate(ctx.world, 1.0, self.now);
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
        self.controls.begin(frame.input);
        let cam = self.camera;
        self.fill_hud(cam.as_ref(), frame.size);
        let ui_events = self.hud.frame(self.controls.input(), frame.size);
        self.ui_events(ctx.world, ctx.agent, &ui_events);
        let map = Arc::clone(self.state.content_arc());
        let Some(scene) = &self.scene else { return };
        let actions = self.controls.act(ui_events.consumed, frame.dt_s.min(0.25), frame.size, &scene.geo, &map.map);
        for a in actions {
            self.act(ctx.world, ctx.agent, a);
        }
        self.controls.sync_camera(frame.camera, frame.agent_view);
        self.camera = *frame.camera;
        self.hover(ctx.world);
        if let Some(scene) = &mut self.scene {
            scene.animate(ctx.world, alpha, self.now);
        }
        if let Some(cam) = &self.camera {
            self.sfx.listen(cam);
        }
        let fires = self.fires();
        self.sfx.fires(&fires);
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
            scene.animate(ctx.world, 1.0, self.now);
        }
    }

    fn tools(&self) -> Vec<ToolDef> {
        tools::defs()
    }

    fn call_tool(&mut self, ctx: &mut Ctx, name: &str, args: &Json) -> ToolOutput {
        tools::call(self, ctx, name, args)
    }

    fn status(&self) -> Json {
        self.status_json()
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
        self.speed = c.speed;
        self.events = c.events;
        self.pending = c.pending;
        self.now = c.now;
        self.pres_ticks = c.pres_ticks;
        self.log = c.log;
        // Presentation caches are rebuilt on the next frame.
        self.hover_key = None;
    }
}

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
//! stepped.

use crate::controls::{Action, Controls};
use crate::hud::Hud;
use crate::scene::{CursorLook, Scene};
use crate::{looks::palette, tools};
use sepulcra::script::{Journal, Player};
use sepulcra::sim::DustSource;
use sepulcra::{CellId, Command, Content, Event, Outcome, Rejection, State, TICK_HZ};
use specula::game::{Ctx, Frame, Game};
use specula::world::{EnvPatch, World};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_protocol::{ToolDef, ToolOutput};
use std::collections::VecDeque;
use std::sync::Arc;

const MESSAGE_S: f32 = 6.0;
const LOG_LEN: usize = 32;
pub const SPEEDS: [u32; 3] = [1, 2, 4];

/// The game, as the engine hosts it.
pub struct SepulcraGame {
    pub(crate) state: State,
    /// Every applied command and its tick: the run's recording.
    pub(crate) journal: Journal,
    /// A build order being followed (`play_new` with a layout).
    pub(crate) autoplay: Option<(String, Player)>,
    pub(crate) scene: Option<Scene>,
    controls: Controls,
    hud: Hud,
    /// Stone index, or `None` for the wall.
    selected: Option<u8>,
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
    message_until: f32,
    /// Recent messages with their tick, for agents (`play_state`).
    pub(crate) log: VecDeque<(u32, String)>,
}

impl SepulcraGame {
    pub fn new(content: Arc<Content>, seed: u64) -> SepulcraGame {
        SepulcraGame {
            state: State::new(content, seed),
            journal: Journal::new(),
            autoplay: None,
            scene: None,
            controls: Controls::default(),
            hud: Hud { speed: 1, ..Hud::default() },
            selected: Some(0),
            paused: false,
            speed: 1,
            events: Vec::new(),
            pending: Vec::new(),
            now: 0.0,
            pres_ticks: 0,
            message_until: 0.0,
            log: VecDeque::new(),
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Applies a command now (at the current tick) and journals it.
    pub fn command(&mut self, world: &mut World, cmd: Command) -> Result<(), Rejection> {
        let tick = self.state.tick();
        self.state.apply(&cmd, &mut self.pending)?;
        self.journal.push((tick, cmd));
        self.messages_from_pending();
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
        self.rebuild(world, agent);
    }

    fn rebuild(&mut self, world: &mut World, agent: &str) {
        let mut scene = Scene::build(world, agent, &self.state);
        scene.sync(world, &self.state, &[], self.now);
        scene.animate(world, 1.0, self.now);
        self.scene = Some(scene);
    }

    pub(crate) fn say(&mut self, msg: String) {
        self.hud.message = msg.clone();
        self.message_until = self.now + MESSAGE_S;
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
                Event::Won => "Victory! The graveyard holds.".into(),
                Event::Lost => "Defeat: the dead walk.".into(),
                _ => continue,
            };
            self.say(msg);
        }
    }

    pub(crate) fn content(&self) -> Arc<Content> {
        Arc::clone(self.state.content_arc())
    }

    fn selected_label(&self) -> String {
        let content = self.state.content();
        match self.selected.and_then(|i| content.stones.get(usize::from(i))) {
            Some(def) => format!("{} ({})", def.name, def.tiers[0].cost),
            None => format!("Wall ({})", content.rules.wall.cost),
        }
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
            Rejection::Occupied => "a stone is there (U upgrades it)".into(),
            Rejection::NotBuildable => "stones go on raised fragments and cliff ledges".into(),
            Rejection::NotWallable => "walls go on top-face lanes".into(),
            Rejection::EnemyInTheWay { .. } => "an enemy is in the way".into(),
            Rejection::MaxTier => "already tier III".into(),
            r => r.code().replace('_', " "),
        };
        format!("Can't {what}: {hint}")
    }

    fn act(&mut self, world: &mut World, agent: &str, action: Action) {
        let cmd = match action {
            Action::Select(i) => {
                let n = self.state.content().stones.len();
                self.selected = i.filter(|&i| usize::from(i) < n).or(if i.is_some() { self.selected } else { None });
                return;
            }
            Action::SelectNext => {
                let n = self.state.content().stones.len() as u8;
                self.selected = match self.selected {
                    Some(i) if i + 1 < n => Some(i + 1),
                    Some(_) => None,
                    None => Some(0),
                };
                return;
            }
            Action::Build(cell) => self.build_command(cell),
            Action::Upgrade(cell) => Command::Upgrade { cell },
            Action::Sell(cell) => Command::Sell { cell },
            Action::Unwall(cell) => Command::RemoveWall { cell },
            Action::CallWave => Command::CallWave,
            Action::TogglePause => {
                self.paused = !self.paused;
                return;
            }
            Action::ToggleHelp => {
                self.hud.help = !self.hud.help;
                return;
            }
            Action::CycleSpeed => {
                let i = SPEEDS.iter().position(|&s| s == self.speed).unwrap_or(0);
                self.speed = SPEEDS[(i + 1) % SPEEDS.len()];
                return;
            }
            Action::NewGame => {
                if self.state.outcome() != Outcome::Running {
                    let seed = self.state.seed().wrapping_add(1);
                    let content = self.content();
                    self.restart(world, agent, content, seed, None);
                } else {
                    self.say("The run is still going (N starts a new game after the end)".into());
                }
                return;
            }
        };
        if let Err(r) = self.command(world, cmd) {
            let msg = self.explain(&cmd, r);
            self.say(msg);
        }
    }

    fn hover_text(&self, cell: CellId) -> String {
        let content = self.state.content();
        let map = &content.map;
        let mut t = map.cell_name(cell);
        if let Some(s) = self.state.stone(cell) {
            let def = content.stone(s.kind);
            t.push_str(&format!(" {} {}", def.name, ["I", "II", "III"][usize::from(s.tier.min(2))]));
            if let Some(next) = def.tiers.get(usize::from(s.tier) + 1) {
                t.push_str(&format!(" (U +{})", next.cost));
            }
            t.push_str(&format!(" (X +{})", s.invested * content.rules.refund_percent / 100));
        } else if let Some(w) = self.state.wall(cell) {
            t.push_str(&format!(" wall {}/{} HP (R)", w.hp / 1000, w.max_hp / 1000));
        }
        t
    }

    /// Cursor and range ring for the hovered cell.
    fn hover(&mut self, world: &mut World) {
        let Some(scene) = self.scene.as_mut() else { return };
        let content = Arc::clone(self.state.content_arc());
        let Some(cell) = self.controls.hover else {
            scene.show_cursor(world, &self.state, CursorLook::Hidden);
            scene.show_range(world, &self.state, None);
            self.hud.hover.clear();
            return;
        };
        let cmd = match self.selected.filter(|&i| usize::from(i) < content.stones.len()) {
            Some(i) => Command::PlaceStone { cell, stone: sepulcra::StoneKind(i) },
            None => Command::PlaceWall { cell },
        };
        let ok = self.state.check(&cmd).is_ok();
        scene.show_cursor(world, &self.state, if ok { CursorLook::Valid(cell) } else { CursorLook::Invalid(cell) });
        let ring = match (self.state.stone(cell), cmd) {
            (Some(s), _) => content.stone(s.kind).tiers.get(usize::from(s.tier)).map(|t| (cell, t.range as f32 / 1000.0)),
            (None, Command::PlaceStone { stone, .. }) if Scene::buildable(&self.state, cell) => {
                Some((cell, content.stone(stone).tiers[0].range as f32 / 1000.0))
            }
            _ => None,
        };
        scene.show_range(world, &self.state, ring);
        let text = self.hover_text(cell);
        self.hud.hover = text;
    }

    pub(crate) fn status_json(&self) -> Json {
        let s = &self.state;
        Json::obj([
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
        sun_color: Some(palette::MOON),
        sun_intensity: Some(1.3),
        ambient_color: Some(palette::MIST),
        ambient_intensity: Some(2.2),
        exposure: Some(1.1),
        tonemap: Some(true),
        background: Some(palette::SKY),
    };
    if let Err(e) = world.set_env(&patch) {
        eprintln!("sepulcra: lighting: {e}");
    }
}

impl Game for SepulcraGame {
    fn name(&self) -> &str {
        "sepulcra"
    }

    fn tick_hz(&self) -> u32 {
        TICK_HZ
    }

    fn start(&mut self, ctx: &mut Ctx) {
        night(ctx.world);
        self.controls.enable_gamepads();
        self.rebuild(ctx.world, ctx.agent);
        self.say(format!("Seed {}: build before wave 1 (F1 for help)", self.state.seed()));
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
        if !self.paused {
            // Flashes keep fading after the run ends (no more steps, but time still passes).
            self.pres_ticks += u64::from(self.speed).max(stepped);
        }
        self.now = self.pres_ticks as f32 / TICK_HZ as f32;
        if let Some(scene) = &mut self.scene {
            scene.sync(ctx.world, &self.state, &events, self.now);
            // Everything where it is at this tick: correct for agents' sight_frame even when
            // ticks run without window frames in between (time_control step, headless engines).
            // A window frame then interpolates from here.
            scene.animate(ctx.world, 1.0, self.now);
        }
        self.events = events;
    }

    fn frame(&mut self, ctx: &mut Ctx, frame: &mut Frame) {
        let alpha = if self.paused { 1.0 } else { frame.alpha };
        // Between ticks: interpolate presentation time like positions (alpha of the next tick).
        let sub = if self.paused { 0.0 } else { (alpha - 1.0) * self.speed as f32 };
        self.now = ((self.pres_ticks as f32) + sub).max(0.0) / TICK_HZ as f32;
        let Some(scene) = &self.scene else { return };
        let map = Arc::clone(self.state.content_arc());
        let actions = self.controls.frame(frame.input, frame.dt_s.min(0.25), frame.size, &scene.geo, &map.map);
        for a in actions {
            self.act(ctx.world, ctx.agent, a);
        }
        self.controls.sync_camera(frame.camera);
        self.hover(ctx.world);
        if let Some(scene) = &mut self.scene {
            scene.animate(ctx.world, alpha, self.now);
        }
        if self.now > self.message_until {
            self.hud.message.clear();
        }
        self.hud.read(&self.state);
        self.hud.selected = self.selected_label();
        self.hud.paused = self.paused;
        self.hud.speed = self.speed;
        *frame.title = Some(self.hud.title());
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
}

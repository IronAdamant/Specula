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
use sepulcra::sim::{DustSource, Stone, Wall};
use sepulcra::{CellId, Command, Content, Event, Outcome, Rejection, State, TICK_HZ};
use specula::game::{Ctx, Frame, Game};
use specula::effects::{BloomPatch, FogPatch, GradingFields, GradingPatch, ShadowPatch, SkyPatch, SsaoPatch};
use specula::world::{EnvPatch, World};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_protocol::{ToolDef, ToolOutput};
use std::any::Any;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MESSAGE_S: f32 = 6.0;
/// A window frame this recent means another follows the next tick.
const FRAMED_WITHIN: Duration = Duration::from_millis(250);
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
    /// When `frame` last ran (window frames follow ticks, so ticks needn't move things too).
    last_frame: Option<Instant>,
    /// What the HUD showed last (the title is rebuilt only when it changes).
    shown_hud: Option<Hud>,
    /// The hovered cell and what stood there when its text was made.
    hover_key: Option<(CellId, Option<Stone>, Option<Wall>)>,
    /// The selection the HUD label was made for.
    label_for: Option<Option<u8>>,
    /// Recent messages with their tick, for agents (`play_state`).
    pub(crate) log: VecDeque<(u32, String)>,
}

/// Everything a game tool can change, for [`Game::checkpoint`]: the simulation, the recording,
/// the scene with its cached entity ids, and what the player sees.
struct Checkpoint {
    state: State,
    journal: Journal,
    autoplay: Option<(String, Player)>,
    scene: Option<Scene>,
    selected: Option<u8>,
    paused: bool,
    speed: u32,
    events: Vec<Event>,
    pending: Vec<Event>,
    now: f32,
    pres_ticks: u64,
    message_until: f32,
    hud: Hud,
    log: VecDeque<(u32, String)>,
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
            last_frame: None,
            shown_hud: None,
            hover_key: None,
            label_for: None,
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
        self.label_for = None;
        self.hover_key = None;
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
            self.hover_key = None;
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
        let key = (cell, self.state.stone(cell).copied(), self.state.wall(cell).copied());
        if self.hover_key != Some(key) {
            self.hover_key = Some(key);
            self.hud.hover = self.hover_text(cell);
        }
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
        // Flashes keep fading after the run ends (no more steps, but time still passes).
        let advance = if self.paused { 0 } else { u64::from(self.speed).max(stepped) };
        self.pres_ticks += advance;
        self.now = self.pres_ticks as f32 / TICK_HZ as f32;
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
        // Frames draw the span between the last two ticks: positions at prev..cur by alpha, and
        // presentation time to match. When either the game or the engine clock is paused there is
        // no span: draw the newest tick (alpha 1), as play_state reports it.
        let still = self.paused || ctx.clock.paused();
        let alpha = if still { 1.0 } else { frame.alpha };
        let sub = (alpha - 1.0) * self.speed as f32;
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
        if self.label_for != Some(self.selected) {
            self.label_for = Some(self.selected);
            self.hud.selected = self.selected_label();
        }
        self.hud.paused = self.paused;
        self.hud.speed = self.speed;
        // The title only when something on it changed (no string building per frame).
        if self.shown_hud.as_ref() != Some(&self.hud) {
            *frame.title = Some(self.hud.title());
            self.shown_hud = Some(self.hud.clone());
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
            paused: self.paused,
            speed: self.speed,
            events: self.events.clone(),
            pending: self.pending.clone(),
            now: self.now,
            pres_ticks: self.pres_ticks,
            message_until: self.message_until,
            hud: self.hud.clone(),
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
        self.paused = c.paused;
        self.speed = c.speed;
        self.events = c.events;
        self.pending = c.pending;
        self.now = c.now;
        self.pres_ticks = c.pres_ticks;
        self.message_until = c.message_until;
        self.hud = c.hud;
        self.log = c.log;
        // Presentation caches are rebuilt on the next frame.
        self.shown_hud = None;
        self.hover_key = None;
        self.label_for = None;
    }
}

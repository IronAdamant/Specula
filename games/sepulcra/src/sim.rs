//! The simulation: state, commands, events and the fixed tick.
//!
//! [`State::step`] is the whole game: apply the player's commands, then advance one tick. Order
//! within a tick (fixed, documented because replays depend on it):
//!
//! 1. Commands, in the order given ([`State::apply`]).
//! 2. The wave timer: start the next wave when it runs out.
//! 3. Spawns that are due, then boss summons.
//! 4. Enemies move, by id: ground enemies follow the flow field (or break walls when no route is
//!    open), runners roll for lane jumps, flyers circle and dive; arrivals at the graveyard cost
//!    tombstones.
//! 5. Stones fire, by cell id.
//! 6. Projectiles fly and hit, by id.
//! 7. The dead are removed; wave-cleared, won and lost are decided.
//!
//! Everything in [`State`] is integers, ordered containers and seeded RNG streams, so the same
//! seed and the same commands give the same [`State::hash`] on every machine.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use specula_core::hash::StableHasher;
use specula_core::rng::Rng;

use crate::content::{Attack, Content, EnemyDef, EnemyKind, Policy, SpawnPoint, StoneDef, StoneKind, Tier};
use crate::fixed::{Pos, TICK_HZ};
use crate::map::{Cell, CellId, CellKind, Face};
use crate::path::{Class, Fields, Route, UNREACHABLE};

pub type EnemyId = u32;
pub type ProjectileId = u32;

/// Height of a stone's muzzle above its cell, where projectiles start.
const MUZZLE: i32 = 1500;

/// Player input. Commands address cells (one stone per cell), so agents, replays and the UI all
/// speak the same language. Recorded with their tick, they replay a run exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    PlaceStone { cell: CellId, stone: StoneKind },
    /// Carve the next tier of runes into the stone on `cell`.
    Upgrade { cell: CellId },
    /// Refunds 70 % of everything spent on the stone.
    Sell { cell: CellId },
    PlaceWall { cell: CellId },
    /// Refunds 70 % of the wall's cost, scaled by its remaining HP.
    RemoveWall { cell: CellId },
    /// Start the next wave now, for bonus dust.
    CallWave,
}

/// Why a command was refused. Rejected commands change nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rejection {
    GameOver,
    UnknownCell,
    UnknownStone,
    /// Stones go on fragment and ledge cells only.
    NotBuildable,
    Occupied,
    NoStone,
    MaxTier,
    NotEnoughDust { need: i32, have: i32 },
    /// Walls go on top-face lane cells only.
    NotWallable,
    WallExists,
    NoWall,
    /// A ground enemy stands on or is stepping into the cell.
    EnemyInTheWay { enemy: EnemyId },
    NoMoreWaves,
}

impl Rejection {
    /// A stable machine-readable code.
    pub fn code(&self) -> &'static str {
        match self {
            Rejection::GameOver => "game_over",
            Rejection::UnknownCell => "unknown_cell",
            Rejection::UnknownStone => "unknown_stone",
            Rejection::NotBuildable => "not_buildable",
            Rejection::Occupied => "occupied",
            Rejection::NoStone => "no_stone",
            Rejection::MaxTier => "max_tier",
            Rejection::NotEnoughDust { .. } => "not_enough_dust",
            Rejection::NotWallable => "not_wallable",
            Rejection::WallExists => "wall_exists",
            Rejection::NoWall => "no_wall",
            Rejection::EnemyInTheWay { .. } => "enemy_in_the_way",
            Rejection::NoMoreWaves => "no_more_waves",
        }
    }
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rejection::NotEnoughDust { need, have } => write!(f, "not_enough_dust (need {need}, have {have})"),
            Rejection::EnemyInTheWay { enemy } => write!(f, "enemy_in_the_way (enemy {enemy})"),
            r => f.write_str(r.code()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    Running,
    Won,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DustSource {
    Kill,
    Income,
    EarlyCall,
    Refund,
}

/// What happened during a step, for presentation, audio and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Event {
    WaveStarted { wave: u16, early: bool },
    WaveCleared { wave: u16 },
    Spawned { enemy: EnemyId, kind: EnemyKind, pos: Pos },
    /// A runner leapt a cracked ledge to the neighbouring lane.
    Jumped { enemy: EnemyId, from: CellId, to: CellId },
    /// An enemy with no open route started breaking a wall.
    AttackingWall { enemy: EnemyId, wall: CellId },
    Shot { stone: CellId, projectile: ProjectileId, target: EnemyId },
    /// A pulse stone fired at every eligible enemy in range.
    Pulsed { stone: CellId, affected: u16 },
    Hit { projectile: ProjectileId, enemy: EnemyId, damage: i32 },
    /// A splash projectile landed.
    Exploded { projectile: ProjectileId, pos: Pos },
    /// A bolt arrived after its target had died.
    Missed { projectile: ProjectileId },
    Died { enemy: EnemyId, kind: EnemyKind, pos: Pos },
    DustGained { amount: i32, source: DustSource },
    DustSpent { amount: i32 },
    StonePlaced { cell: CellId, stone: StoneKind },
    StoneUpgraded { cell: CellId, tier: u8 },
    StoneSold { cell: CellId, refund: i32 },
    WallPlaced { cell: CellId },
    WallRemoved { cell: CellId, refund: i32 },
    /// Emitted when a wall's HP crosses 75 %, 50 % or 25 %.
    WallDamaged { cell: CellId, hp: i32, max_hp: i32 },
    WallBroken { cell: CellId },
    TombstonesLost { enemy: EnemyId, kind: EnemyKind, lost: i32, remaining: i32 },
    Won,
    Lost,
    /// Command `index` of this step was refused.
    Rejected { index: u16, reason: Rejection },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stone {
    pub kind: StoneKind,
    /// 0 = tier I.
    pub tier: u8,
    /// Dust spent on it so far (selling refunds a share).
    pub invested: i32,
    pub ready_at: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Wall {
    pub hp: i32,
    pub max_hp: i32,
    pub cost: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Motion {
    /// Walking (or leaping, when `leap`) the edge `from → to`; `progress` of `len` mm.
    /// Standing still (spawning, attacking a wall) is `from == to` with `len == 0`.
    Ground { from: CellId, to: CellId, progress: i32, len: i32, leap: bool },
    /// Flying `from → to`. Circling the ring while `hops_left > 0` (`ring` is the index of `to`),
    /// then one dive at the graveyard.
    Air { from: Pos, to: Pos, progress: i32, len: i32, ring: u16, dir: i8, hops_left: u16, diving: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Enemy {
    pub id: EnemyId,
    pub kind: EnemyKind,
    pub wave: u16,
    /// Milli-HP.
    pub hp: i32,
    pub max_hp: i32,
    pub pos: Pos,
    pub motion: Motion,
    /// Current slow (permille) while `tick < slow_until`.
    pub slow: i32,
    pub slow_until: u32,
    /// The wall it is breaking this tick.
    pub attacking: Option<CellId>,
    /// Distance left to the graveyard, in mm (what "first" targeting sorts by).
    pub remaining: i32,
    pub alive: bool,
    carry: i64,
    next_summon: u32,
}

impl Enemy {
    /// The cell it is nearer to (ground enemies only).
    pub fn cell(&self) -> Option<CellId> {
        match self.motion {
            Motion::Ground { from, to, progress, len, .. } => Some(if progress * 2 < len { from } else { to }),
            Motion::Air { .. } => None,
        }
    }

    /// The face it is on; `None` in the air.
    pub fn face(&self, content: &Content) -> Option<Face> {
        self.cell().map(|c| content.map.cell(c).face)
    }

    pub fn is_slowed(&self, tick: u32) -> bool {
        tick < self.slow_until
    }

    /// Metres, for presentation.
    pub fn world_pos_f32(&self) -> [f32; 3] {
        self.pos.to_f32()
    }

    fn touches(&self, cell: CellId) -> bool {
        matches!(self.motion, Motion::Ground { from, to, .. } if from == cell || to == cell)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Projectile {
    pub id: ProjectileId,
    pub stone: StoneKind,
    pub tier: u8,
    pub origin: Pos,
    pub pos: Pos,
    /// Where it is heading: the target's position (bolts follow it) or the landing point (lobs).
    pub aim: Pos,
    pub target: Option<EnemyId>,
    pub homing: bool,
    done: bool,
}

impl Projectile {
    pub fn world_pos_f32(&self) -> [f32; 3] {
        self.pos.to_f32()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WaveStats {
    pub started_at: Option<u32>,
    pub cleared_at: Option<u32>,
    pub spawned: u32,
    pub killed: u32,
    pub leaked: u32,
    pub tombstones_lost: i32,
    pub dust_from_kills: i32,
    pending: u32,
    alive: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SpawnAt {
    Ground(CellId),
    Air { entry: u16, dir: i8, hops: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Pending {
    tick: u32,
    seq: u32,
    enemy: EnemyKind,
    wave: u16,
    at: SpawnAt,
}

/// The whole game state: the shared, immutable [`Content`] plus the world that changes.
/// Cheap to clone (content is shared), so "what if" previews and rollbacks are just clones.
#[derive(Clone, Debug)]
pub struct State {
    content: Arc<Content>,
    w: World,
}

/// Everything that changes during a run. Split from [`State`] so the tick can borrow the
/// content and the world separately: no `Arc` is touched per tick (a tick costs well under a
/// microsecond, so in parallel sweeps even one contended reference-count update per tick
/// doubled the run time).
#[derive(Clone, Debug)]
struct World {
    seed: u64,
    tick: u32,
    wave: u16,
    next_wave_tick: Option<u32>,
    dust: i32,
    tombstones: i32,
    outcome: Outcome,
    rng_waves: Rng,
    rng_jump: Rng,
    rng_drops: Rng,
    next_enemy: u32,
    next_projectile: u32,
    next_seq: u32,
    pending: Vec<Pending>,
    enemies: Vec<Enemy>,
    stones: BTreeMap<CellId, Stone>,
    walls: BTreeMap<CellId, Wall>,
    projectiles: Vec<Projectile>,
    wave_stats: Vec<WaveStats>,
    // Derived from the state above (so not hashed), refreshed eagerly when it changes.
    blocked: Vec<bool>,
    fields: Fields,
    rooted: Vec<bool>,
}

/// `step(&mut state, &commands)`: the pure-function form of [`State::step`].
pub fn step(state: &mut State, commands: &[Command]) -> Vec<Event> {
    state.step(commands)
}

impl State {
    pub fn new(content: Arc<Content>, seed: u64) -> State {
        let n = content.map.cells().len();
        let blocked = vec![false; n];
        let fields = Fields::build(&content.map, &blocked, content.rules.wall.breach_penalty);
        let w = World {
            seed,
            tick: 0,
            wave: 0,
            next_wave_tick: Some(content.rules.build_ticks),
            dust: content.rules.start_dust,
            tombstones: content.rules.tombstones,
            outcome: Outcome::Running,
            rng_waves: Rng::named(seed, "waves"),
            rng_jump: Rng::named(seed, "lane_jump"),
            rng_drops: Rng::named(seed, "drops"),
            next_enemy: 1,
            next_projectile: 1,
            next_seq: 0,
            pending: Vec::new(),
            enemies: Vec::new(),
            stones: BTreeMap::new(),
            walls: BTreeMap::new(),
            projectiles: Vec::new(),
            wave_stats: vec![WaveStats::default(); content.waves.len()],
            blocked,
            fields,
            rooted: vec![false; n],
        };
        State { content, w }
    }

    // ---- read access (presentation, agents, tests) ----

    pub fn content(&self) -> &Content {
        &self.content
    }

    pub fn content_arc(&self) -> &Arc<Content> {
        &self.content
    }

    pub fn seed(&self) -> u64 {
        self.w.seed
    }

    pub fn tick(&self) -> u32 {
        self.w.tick
    }

    /// Waves started so far (1-based number of the current wave; 0 before the first).
    pub fn wave(&self) -> u16 {
        self.w.wave
    }

    pub fn waves_total(&self) -> u16 {
        self.content.waves.len() as u16
    }

    /// When the next wave starts on its own (`None` after the last has started).
    pub fn next_wave_tick(&self) -> Option<u32> {
        self.w.next_wave_tick
    }

    pub fn dust(&self) -> i32 {
        self.w.dust
    }

    pub fn tombstones(&self) -> i32 {
        self.w.tombstones
    }

    pub fn outcome(&self) -> Outcome {
        self.w.outcome
    }

    /// Living enemies, sorted by id.
    pub fn enemies(&self) -> &[Enemy] {
        &self.w.enemies
    }

    pub fn enemy(&self, id: EnemyId) -> Option<&Enemy> {
        self.w.enemy_index(id).map(|i| &self.w.enemies[i])
    }

    pub fn stones(&self) -> impl Iterator<Item = (CellId, &Stone)> {
        self.w.stones.iter().map(|(c, s)| (*c, s))
    }

    pub fn stone(&self, cell: CellId) -> Option<&Stone> {
        self.w.stones.get(&cell)
    }

    pub fn walls(&self) -> impl Iterator<Item = (CellId, &Wall)> {
        self.w.walls.iter().map(|(c, w)| (*c, w))
    }

    pub fn wall(&self, cell: CellId) -> Option<&Wall> {
        self.w.walls.get(&cell)
    }

    pub fn projectiles(&self) -> &[Projectile] {
        &self.w.projectiles
    }

    /// Per wave (index 0 = wave 1).
    pub fn wave_stats(&self) -> &[WaveStats] {
        &self.w.wave_stats
    }

    /// Lane cells slowed by roots (nature, from the wave in `stones.json`).
    pub fn is_rooted(&self, cell: CellId) -> bool {
        self.w.rooted[cell.index()]
    }

    /// The current flow fields (for debug overlays and tests).
    pub fn fields(&self) -> &Fields {
        &self.w.fields
    }

    /// Enemies scheduled but not yet spawned.
    pub fn pending_spawns(&self) -> usize {
        self.w.pending.len()
    }

    /// Would `cmd` be accepted right now? Changes nothing (for UI previews).
    pub fn check(&self, cmd: &Command) -> Result<(), Rejection> {
        self.w.check(&self.content, cmd)
    }

    /// Applies one command now, without advancing time (building during active pause).
    /// Applying at tick T and then stepping is identical to `step` at tick T with the command.
    pub fn apply(&mut self, cmd: &Command, ev: &mut Vec<Event>) -> Result<(), Rejection> {
        self.w.apply(&self.content, cmd, ev)
    }

    /// Applies `commands` (refused ones become [`Event::Rejected`]), then advances one tick.
    /// After the game is won or lost, time stops and every command is refused.
    pub fn step(&mut self, commands: &[Command]) -> Vec<Event> {
        self.w.step(&self.content, commands)
    }

    /// A 64-bit hash of the entire simulation state (content included), in a fixed order.
    /// Same seed + same commands → same hash every tick, on every machine.
    pub fn hash(&self) -> u64 {
        self.w.hash(&self.content)
    }

    /// The map with stones (their glyph), walls (`=`) and ground enemies (their glyph, or a
    /// count when several share a cell) drawn in, under a one-line status header.
    pub fn ascii(&self) -> String {
        self.w.ascii(&self.content)
    }
}

impl World {
    // ---- commands ----

    /// Would `cmd` be accepted right now? Changes nothing (for UI previews).
    fn check(&self, content: &Content, cmd: &Command) -> Result<(), Rejection> {
        if self.outcome != Outcome::Running {
            return Err(Rejection::GameOver);
        }
        let map = &content.map;
        let cell_of = |c: CellId| if c.index() < map.cells().len() { Ok(map.cell(c)) } else { Err(Rejection::UnknownCell) };
        match *cmd {
            Command::PlaceStone { cell, stone } => {
                let c = cell_of(cell)?;
                let def = content.stones.get(usize::from(stone.0)).ok_or(Rejection::UnknownStone)?;
                if !c.kind.buildable() {
                    return Err(Rejection::NotBuildable);
                }
                if self.stones.contains_key(&cell) {
                    return Err(Rejection::Occupied);
                }
                self.afford(def.tiers[0].cost)
            }
            Command::Upgrade { cell } => {
                cell_of(cell)?;
                let s = self.stones.get(&cell).ok_or(Rejection::NoStone)?;
                let tiers = &content.stone(s.kind).tiers;
                let next = tiers.get(usize::from(s.tier) + 1).ok_or(Rejection::MaxTier)?;
                self.afford(next.cost)
            }
            Command::Sell { cell } => {
                cell_of(cell)?;
                self.stones.get(&cell).map(|_| ()).ok_or(Rejection::NoStone)
            }
            Command::PlaceWall { cell } => {
                let c = cell_of(cell)?;
                if !c.kind.wallable() {
                    return Err(Rejection::NotWallable);
                }
                if self.walls.contains_key(&cell) {
                    return Err(Rejection::WallExists);
                }
                if let Some(e) = self.enemies.iter().find(|e| e.alive && e.touches(cell)) {
                    return Err(Rejection::EnemyInTheWay { enemy: e.id });
                }
                self.afford(content.rules.wall.cost)
            }
            Command::RemoveWall { cell } => {
                cell_of(cell)?;
                self.walls.get(&cell).map(|_| ()).ok_or(Rejection::NoWall)
            }
            Command::CallWave => {
                if usize::from(self.wave) >= content.waves.len() {
                    return Err(Rejection::NoMoreWaves);
                }
                Ok(())
            }
        }
    }

    fn afford(&self, cost: i32) -> Result<(), Rejection> {
        if cost > self.dust {
            Err(Rejection::NotEnoughDust { need: cost, have: self.dust })
        } else {
            Ok(())
        }
    }

    /// Applies one command now, without advancing time (building during active pause).
    /// Applying at tick T and then stepping is identical to `step` at tick T with the command.
    fn apply(&mut self, content: &Content, cmd: &Command, ev: &mut Vec<Event>) -> Result<(), Rejection> {
        self.check(content, cmd)?;
        let rules = &content.rules;
        match *cmd {
            Command::PlaceStone { cell, stone } => {
                let cost = content.stone(stone).tiers[0].cost;
                self.spend(cost, ev);
                self.stones.insert(cell, Stone { kind: stone, tier: 0, invested: cost, ready_at: self.tick });
                ev.push(Event::StonePlaced { cell, stone });
                self.refresh_roots(content);
            }
            Command::Upgrade { cell } => {
                let s = self.stones[&cell];
                let cost = content.stone(s.kind).tiers[usize::from(s.tier) + 1].cost;
                self.spend(cost, ev);
                let s = self.stones.get_mut(&cell).expect("checked");
                s.tier += 1;
                s.invested += cost;
                ev.push(Event::StoneUpgraded { cell, tier: s.tier });
                self.refresh_roots(content);
            }
            Command::Sell { cell } => {
                let s = self.stones.remove(&cell).expect("checked");
                let refund = s.invested * rules.refund_percent / 100;
                self.gain(refund, DustSource::Refund, ev);
                ev.push(Event::StoneSold { cell, refund });
                self.refresh_roots(content);
            }
            Command::PlaceWall { cell } => {
                self.spend(rules.wall.cost, ev);
                self.walls.insert(cell, Wall { hp: rules.wall.hp, max_hp: rules.wall.hp, cost: rules.wall.cost });
                ev.push(Event::WallPlaced { cell });
                self.set_blocked(content, cell, true);
            }
            Command::RemoveWall { cell } => {
                let w = self.walls.remove(&cell).expect("checked");
                let refund = (i64::from(w.cost) * i64::from(rules.refund_percent) * i64::from(w.hp.max(0))
                    / (100 * i64::from(w.max_hp.max(1)))) as i32;
                self.gain(refund, DustSource::Refund, ev);
                ev.push(Event::WallRemoved { cell, refund });
                self.set_blocked(content, cell, false);
            }
            Command::CallWave => self.start_wave(content, true, ev),
        }
        Ok(())
    }

    fn spend(&mut self, amount: i32, ev: &mut Vec<Event>) {
        self.dust -= amount;
        if amount > 0 {
            ev.push(Event::DustSpent { amount });
        }
    }

    fn gain(&mut self, amount: i32, source: DustSource, ev: &mut Vec<Event>) {
        if amount > 0 {
            self.dust += amount;
            ev.push(Event::DustGained { amount, source });
        }
    }

    fn set_blocked(&mut self, content: &Content, cell: CellId, on: bool) {
        self.blocked[cell.index()] = on;
        self.fields = Fields::build(&content.map, &self.blocked, content.rules.wall.breach_penalty);
    }

    fn refresh_roots(&mut self, content: &Content) {
        let nat = content.rules.nature;
        self.rooted.iter_mut().for_each(|r| *r = false);
        if self.wave < nat.from_wave || nat.slow == 0 {
            return;
        }
        let r2 = i64::from(nat.radius) * i64::from(nat.radius);
        for (cell, s) in &self.stones {
            if s.tier < nat.min_tier {
                continue;
            }
            let c = content.map.cell(*cell);
            for other in content.map.cells() {
                if other.face == c.face && other.kind.walkable(true) && other.pos.dist2(c.pos) <= r2 {
                    self.rooted[other.id.index()] = true;
                }
            }
        }
    }

    // ---- the tick ----

    /// Applies `commands` (refused ones become [`Event::Rejected`]), then advances one tick.
    /// After the game is won or lost, time stops and every command is refused.
    fn step(&mut self, content: &Content, commands: &[Command]) -> Vec<Event> {
        let mut ev = Vec::new();
        for (i, c) in commands.iter().enumerate() {
            if let Err(reason) = self.apply(content, c, &mut ev) {
                ev.push(Event::Rejected { index: i as u16, reason });
            }
        }
        if self.outcome == Outcome::Running {
            if self.next_wave_tick.is_some_and(|t| self.tick >= t) {
                self.start_wave(content, false, &mut ev);
            }
            // One handle for the whole tick (not one per phase or per enemy): parallel sweeps
            // sharing an `Arc<Content>` would otherwise contend on its reference count.
                self.spawn_due(content, &mut ev);
            self.summon(content, &mut ev);
            self.move_enemies(content, &mut ev);
            self.fire_stones(content, &mut ev);
            self.move_projectiles(content, &mut ev);
            self.finish_tick(&mut ev);
            self.tick += 1;
        }
        ev
    }

    fn start_wave(&mut self, content: &Content, early: bool, ev: &mut Vec<Event>) {
        let w = usize::from(self.wave);
        let def = &content.waves[w];
        if early {
            if let Some(t) = self.next_wave_tick {
                let left = i64::from(t.saturating_sub(self.tick));
                let bonus = left * i64::from(content.rules.early_call_milli_per_s) / (i64::from(TICK_HZ) * 1000);
                self.gain(bonus as i32, DustSource::EarlyCall, ev);
            }
        }
        self.wave += 1;
        ev.push(Event::WaveStarted { wave: self.wave, early });
        self.gain(def.income, DustSource::Income, ev);
        self.wave_stats[w].started_at = Some(self.tick);
        for g in &def.groups {
            let edef = content.enemy(g.enemy);
            for k in 0..u32::from(g.count) {
                let point = g.from[self.rng_waves.below(g.from.len() as u32) as usize];
                let at = match point {
                    SpawnPoint::Ground(i) => SpawnAt::Ground(content.map.spawns[usize::from(i)].cell),
                    SpawnPoint::Air(entry) => {
                        let dir = if self.rng_waves.below(2) == 0 { 1 } else { -1 };
                        let hops = self.rng_waves.range_i32(i32::from(edef.circle.0), i32::from(edef.circle.1)) as u16;
                        SpawnAt::Air { entry, dir, hops }
                    }
                };
                self.pending.push(Pending { tick: self.tick + g.start + k * g.every, seq: self.next_seq, enemy: g.enemy, wave: self.wave, at });
                self.next_seq += 1;
                self.wave_stats[w].pending += 1;
            }
        }
        self.pending.sort_by_key(|p| (p.tick, p.seq));
        self.next_wave_tick = (usize::from(self.wave) < content.waves.len()).then(|| self.tick + def.duration);
        if self.wave == content.rules.nature.from_wave {
            self.refresh_roots(content);
        }
    }

    fn spawn_due(&mut self, content: &Content, ev: &mut Vec<Event>) {
        let n = self.pending.iter().take_while(|p| p.tick <= self.tick).count();
        let due: Vec<Pending> = self.pending.drain(..n).collect();
        for p in due {
            self.wave_stats[usize::from(p.wave) - 1].pending -= 1;
            self.spawn(content, p.enemy, p.wave, p.at, ev);
        }
    }

    fn spawn(&mut self, content: &Content, kind: EnemyKind, wave: u16, at: SpawnAt, ev: &mut Vec<Event>) {
        let def = content.enemy(kind);
        let hp = (i64::from(def.hp) * i64::from(content.waves[usize::from(wave) - 1].hp_scale) / 1000).max(1) as i32;
        let motion = match at {
            SpawnAt::Ground(c) => Motion::Ground { from: c, to: c, progress: 0, len: 0, leap: true },
            SpawnAt::Air { entry, dir, hops } => {
                let start = content.map.air.entries[usize::from(entry)];
                self.air_leg(content, content.map.air.ring[usize::from(start)], start, dir, hops)
            }
        };
        let id = self.next_enemy;
        self.next_enemy += 1;
        let mut e = Enemy {
            id,
            kind,
            wave,
            hp,
            max_hp: hp,
            pos: Pos::default(),
            motion,
            slow: 0,
            slow_until: 0,
            attacking: None,
            remaining: 0,
            alive: true,
            carry: 0,
            next_summon: self.tick + def.summon.map_or(0, |s| s.every),
        };
        self.place(content, &mut e);
        ev.push(Event::Spawned { enemy: id, kind, pos: e.pos });
        self.enemies.push(e);
        let s = &mut self.wave_stats[usize::from(wave) - 1];
        s.spawned += 1;
        s.alive += 1;
    }

    fn summon(&mut self, content: &Content, ev: &mut Vec<Event>) {
        let mut calls = Vec::new();
        for e in &mut self.enemies {
            if let (true, Some(s), Some(cell)) = (e.alive, content.enemy(e.kind).summon, e.cell()) {
                if self.tick >= e.next_summon {
                    e.next_summon = self.tick + s.every;
                    calls.push((s, cell, e.wave));
                }
            }
        }
        for (s, cell, wave) in calls {
            for _ in 0..s.count {
                self.spawn(content, s.enemy, wave, SpawnAt::Ground(cell), ev);
            }
        }
    }

    fn air_leg(&self, content: &Content, at: Pos, ring_idx: u16, dir: i8, hops_left: u16) -> Motion {
        let air = &content.map.air;
        if hops_left > 0 {
            let n = air.ring.len() as i32;
            let next = (i32::from(ring_idx) + i32::from(dir)).rem_euclid(n) as u16;
            let to = air.ring[usize::from(next)];
            Motion::Air { from: at, to, progress: 0, len: at.dist(to), ring: next, dir, hops_left: hops_left - 1, diving: false }
        } else {
            let to = air.dive_target;
            Motion::Air { from: at, to, progress: 0, len: at.dist(to), ring: ring_idx, dir, hops_left: 0, diving: true }
        }
    }

    /// Updates an enemy's position and remaining distance from its motion.
    fn place(&self, content: &Content, e: &mut Enemy) {
        let map = &content.map;
        match e.motion {
            Motion::Ground { from, to, progress, len, .. } => {
                e.pos = Pos::lerp(map.cell(from).pos, map.cell(to).pos, progress, len);
                let class = class_of(content.enemy(e.kind));
                let rest = self.fields.remaining(class, to);
                e.remaining = if rest == UNREACHABLE { UNREACHABLE } else { rest.saturating_add(len - progress) };
            }
            Motion::Air { from, to, progress, len, hops_left, diving, .. } => {
                e.pos = Pos::lerp(from, to, progress, len);
                let seg = map.air.ring[0].dist(map.air.ring[1]);
                let dive = if diving { 0 } else { map.air.ring_half };
                e.remaining = (len - progress) + i32::from(hops_left) * seg + dive;
            }
        }
    }

    fn speed_permille(&self, content: &Content, e: &Enemy, climbing: bool) -> i64 {
        let mut f = 1000i64;
        if e.is_slowed(self.tick) {
            f = f * i64::from(1000 - e.slow) / 1000;
        }
        if let Some(c) = e.cell() {
            if self.rooted[c.index()] {
                f = f * i64::from(1000 - content.rules.nature.slow) / 1000;
            }
        }
        if climbing {
            f = f * i64::from(content.enemy(e.kind).climb_speed) / 1000;
        }
        f
    }

    /// Converts this tick's speed into whole millimetres, carrying the remainder.
    fn budget(e: &mut Enemy, speed: i32, permille: i64) -> i32 {
        let den = i64::from(TICK_HZ) * 1000;
        let num = i64::from(speed) * permille + e.carry;
        e.carry = num % den;
        (num / den) as i32
    }

    fn move_enemies(&mut self, content: &Content, ev: &mut Vec<Event>) {
        for i in 0..self.enemies.len() {
            if !self.enemies[i].alive {
                continue;
            }
            let mut e = self.enemies[i];
            let def = content.enemy(e.kind);
            match e.motion {
                Motion::Ground { .. } => self.move_ground(content, &mut e, def, ev),
                Motion::Air { .. } => self.move_air(content, &mut e, def, ev),
            }
            self.place(content, &mut e);
            self.enemies[i] = e;
            if self.outcome != Outcome::Running {
                break;
            }
        }
    }

    fn move_ground(&mut self, content: &Content, e: &mut Enemy, def: &EnemyDef, ev: &mut Vec<Event>) {
        let map = &content.map;
        let class = class_of(def);
        let Motion::Ground { from, to, .. } = e.motion else { return };
        let climbing = map.cell(from).kind.is_climb() || map.cell(to).kind.is_climb();
        let mut left = World::budget(e, def.speed, self.speed_permille(content, e, climbing));
        let was_attacking = e.attacking.take();
        // A few edges per tick at most; the bound only guards against zero-length loops.
        for _ in 0..8 {
            let Motion::Ground { from, to, progress, len, leap } = e.motion else { return };
            if progress < len {
                let adv = left.min(len - progress);
                left -= adv;
                e.motion = Motion::Ground { from, to, progress: progress + adv, len, leap };
                if progress + adv < len {
                    return;
                }
                continue;
            }
            let at = to;
            let kind = map.cell(at).kind;
            if kind == CellKind::Graveyard {
                self.leak(e, def, ev);
                return;
            }
            if !leap && kind == CellKind::Jump && def.jump_chance > 0 {
                if let Some(p) = self.jump_target(content, class, at, def) {
                    if (self.rng_jump.below(1000) as i32) < def.jump_chance {
                        let len = map.cell(at).pos.dist(map.cell(p).pos);
                        e.motion = Motion::Ground { from: at, to: p, progress: 0, len, leap: true };
                        ev.push(Event::Jumped { enemy: e.id, from: at, to: p });
                        continue;
                    }
                }
            }
            match self.fields.route(map, class, &self.blocked, at) {
                Route::Step(n) => {
                    let len = map.links(at).iter().find(|l| l.to == n).map_or(0, |l| l.cost);
                    e.motion = Motion::Ground { from: at, to: n, progress: 0, len, leap: false };
                    if left == 0 {
                        return;
                    }
                }
                Route::Attack(w) => {
                    e.motion = Motion::Ground { from: at, to: at, progress: 0, len: 0, leap: true };
                    e.carry = 0;
                    e.attacking = Some(w);
                    if was_attacking != Some(w) {
                        ev.push(Event::AttackingWall { enemy: e.id, wall: w });
                    }
                    self.damage_wall(content, w, def.wall_damage, ev);
                    return;
                }
                Route::Stuck => {
                    e.motion = Motion::Ground { from: at, to: at, progress: 0, len: 0, leap: true };
                    e.carry = 0;
                    return;
                }
            }
        }
    }

    /// The partner a runner may leap to from jump point `at`: open, reaching the graveyard, and
    /// not more than the enemy's `jump_detour` farther away (any open route beats none).
    fn jump_target(&self, content: &Content, class: Class, at: CellId, def: &EnemyDef) -> Option<CellId> {
        let open = self.fields.open(class);
        let here = open[at.index()];
        let mut best: Option<CellId> = None;
        for &p in content.map.jump_partners(at) {
            let d = open[p.index()];
            if self.blocked[p.index()] || d == UNREACHABLE {
                continue;
            }
            if here != UNREACHABLE && i64::from(d) > i64::from(here) + i64::from(def.jump_detour) {
                continue;
            }
            if best.is_none_or(|b| d < open[b.index()]) {
                best = Some(p);
            }
        }
        best
    }

    fn move_air(&mut self, content: &Content, e: &mut Enemy, def: &EnemyDef, ev: &mut Vec<Event>) {
        let mut left = World::budget(e, def.speed, self.speed_permille(content, e, false));
        for _ in 0..8 {
            let Motion::Air { from, to, progress, len, ring, dir, hops_left, diving } = e.motion else { return };
            if progress < len {
                let adv = left.min(len - progress);
                left -= adv;
                e.motion = Motion::Air { from, to, progress: progress + adv, len, ring, dir, hops_left, diving };
                if progress + adv < len {
                    return;
                }
                continue;
            }
            if diving {
                self.leak(e, def, ev);
                return;
            }
            e.motion = self.air_leg(content, to, ring, dir, hops_left);
        }
    }

    fn leak(&mut self, e: &mut Enemy, def: &EnemyDef, ev: &mut Vec<Event>) {
        e.alive = false;
        let lost = def.tombstones.min(self.tombstones);
        self.tombstones -= lost;
        let s = &mut self.wave_stats[usize::from(e.wave) - 1];
        s.leaked += 1;
        s.alive -= 1;
        s.tombstones_lost += lost;
        ev.push(Event::TombstonesLost { enemy: e.id, kind: e.kind, lost, remaining: self.tombstones });
        if self.tombstones <= 0 && self.outcome == Outcome::Running {
            self.outcome = Outcome::Lost;
            ev.push(Event::Lost);
        }
    }

    fn damage_wall(&mut self, content: &Content, cell: CellId, dmg: i32, ev: &mut Vec<Event>) {
        let Some(w) = self.walls.get_mut(&cell) else { return };
        let quarter = |hp: i32, max: i32| i64::from(hp.max(0)) * 4 / i64::from(max.max(1));
        let before = quarter(w.hp, w.max_hp);
        w.hp -= dmg;
        if w.hp <= 0 {
            self.walls.remove(&cell);
            ev.push(Event::WallBroken { cell });
            self.set_blocked(content, cell, false);
        } else if quarter(w.hp, w.max_hp) != before {
            ev.push(Event::WallDamaged { cell, hp: w.hp, max_hp: w.max_hp });
        }
    }

    fn enemy_index(&self, id: EnemyId) -> Option<usize> {
        self.enemies.binary_search_by_key(&id, |e| e.id).ok()
    }

    /// Range across faces: always within `range` (3D), and when the enemy is on another face
    /// than the stone, the stone must stand within `edge_reach` of the rim those faces share.
    /// Flyers are in open sky: range alone decides.
    fn in_range(&self, content: &Content, c: &Cell, range: i32, e: &Enemy) -> bool {
        if c.pos.dist2(e.pos) > i64::from(range) * i64::from(range) {
            return false;
        }
        match e.face(content) {
            None => true,
            Some(f) if f == c.face => true,
            Some(f) => content.map.rim_distance(c.pos, c.face, f).is_some_and(|d| d <= content.rules.edge_reach),
        }
    }

    fn eligible(&self, content: &Content, c: &Cell, def: &StoneDef, t: &Tier, e: &Enemy) -> bool {
        e.alive && def.targets.hits(content.enemy(e.kind).flies()) && self.in_range(content, c, t.range, e)
    }

    fn pick_target(&self, content: &Content, c: &Cell, def: &StoneDef, t: &Tier) -> Option<usize> {
        let mut best: Option<(i64, usize)> = None;
        for (i, e) in self.enemies.iter().enumerate() {
            if !self.eligible(content, c, def, t, e) {
                continue;
            }
            let rem = i64::from(e.remaining);
            let key = match def.policy {
                Policy::First => rem,
                Policy::Last => -rem,
                Policy::Strongest => -i64::from(e.hp) * (1 << 32) + rem,
                Policy::Closest => c.pos.dist2(e.pos),
            };
            // Enemies are in id order, so a strict `<` gives ties to the lowest id.
            if best.is_none_or(|(b, _)| key < b) {
                best = Some((key, i));
            }
        }
        best.map(|(_, i)| i)
    }

    fn fire_stones(&mut self, content: &Content, ev: &mut Vec<Event>) {
        let ready: Vec<(CellId, Stone)> = self.stones.iter().filter(|(_, s)| self.tick >= s.ready_at).map(|(c, s)| (*c, *s)).collect();
        for (cell, s) in ready {
            let def = content.stone(s.kind);
            let t = def.tiers[usize::from(s.tier)];
            let c = content.map.cell(cell);
            let fired = match def.attack {
                Attack::Pulse => {
                    let mut n = 0u16;
                    for i in 0..self.enemies.len() {
                        if self.eligible(content, c, def, &t, &self.enemies[i]) {
                            n += 1;
                            self.slow_enemy(i, t.slow, t.slow_ticks);
                            self.hurt(content, i, t.damage, ev);
                        }
                    }
                    if n > 0 {
                        ev.push(Event::Pulsed { stone: cell, affected: n });
                    }
                    n > 0
                }
                Attack::Bolt | Attack::Lob => match self.pick_target(content, c, def, &t) {
                    Some(i) => {
                        let target = self.enemies[i];
                        let id = self.next_projectile;
                        self.next_projectile += 1;
                        let origin = c.pos.up(MUZZLE);
                        self.projectiles.push(Projectile {
                            id,
                            stone: s.kind,
                            tier: s.tier,
                            origin,
                            pos: origin,
                            aim: target.pos,
                            target: Some(target.id),
                            homing: def.attack == Attack::Bolt,
                            done: false,
                        });
                        ev.push(Event::Shot { stone: cell, projectile: id, target: target.id });
                        true
                    }
                    None => false,
                },
            };
            if fired {
                self.stones.get_mut(&cell).expect("stone").ready_at = self.tick + t.cooldown;
            }
        }
    }

    fn slow_enemy(&mut self, i: usize, permille: i32, ticks: u32) {
        let tick = self.tick;
        let e = &mut self.enemies[i];
        if permille <= 0 || !e.alive {
            return;
        }
        if tick >= e.slow_until || permille >= e.slow {
            e.slow = permille;
            e.slow_until = e.slow_until.max(tick + ticks);
        }
    }

    fn hurt(&mut self, content: &Content, i: usize, dmg: i32, ev: &mut Vec<Event>) {
        if dmg <= 0 || !self.enemies[i].alive {
            return;
        }
        let e = &mut self.enemies[i];
        e.hp -= dmg;
        if e.hp > 0 {
            return;
        }
        e.alive = false;
        let (id, kind, pos, wave) = (e.id, e.kind, e.pos, e.wave);
        let (lo, hi) = content.enemy(kind).dust;
        let dust = self.rng_drops.range_i32(lo, hi);
        let s = &mut self.wave_stats[usize::from(wave) - 1];
        s.killed += 1;
        s.alive -= 1;
        s.dust_from_kills += dust;
        ev.push(Event::Died { enemy: id, kind, pos });
        self.gain(dust, DustSource::Kill, ev);
    }

    fn move_projectiles(&mut self, content: &Content, ev: &mut Vec<Event>) {
        for k in 0..self.projectiles.len() {
            let mut p = self.projectiles[k];
            let def = content.stone(p.stone);
            let t = def.tiers[usize::from(p.tier)];
            if p.homing {
                if let Some(tid) = p.target {
                    match self.enemy_index(tid) {
                        Some(i) if self.enemies[i].alive => p.aim = self.enemies[i].pos,
                        _ => p.target = None,
                    }
                }
            }
            let (pos, arrived) = p.pos.step_toward(p.aim, t.projectile_speed);
            p.pos = pos;
            if arrived {
                p.done = true;
                self.impact(content, &p, def, &t, ev);
            }
            self.projectiles[k] = p;
        }
        self.projectiles.retain(|p| !p.done);
    }

    fn impact(&mut self, content: &Content, p: &Projectile, def: &StoneDef, t: &Tier, ev: &mut Vec<Event>) {
        if t.splash > 0 {
            ev.push(Event::Exploded { projectile: p.id, pos: p.aim });
            let r2 = i64::from(t.splash) * i64::from(t.splash);
            for i in 0..self.enemies.len() {
                let e = &self.enemies[i];
                if e.alive && def.targets.hits(content.enemy(e.kind).flies()) && e.pos.dist2(p.aim) <= r2 {
                    self.hit(content, i, p.id, t, ev);
                }
            }
            return;
        }
        match p.target.and_then(|id| self.enemy_index(id)) {
            Some(i) if self.enemies[i].alive => self.hit(content, i, p.id, t, ev),
            _ => ev.push(Event::Missed { projectile: p.id }),
        }
    }

    fn hit(&mut self, content: &Content, i: usize, projectile: ProjectileId, t: &Tier, ev: &mut Vec<Event>) {
        ev.push(Event::Hit { projectile, enemy: self.enemies[i].id, damage: t.damage });
        self.slow_enemy(i, t.slow, t.slow_ticks);
        self.hurt(content, i, t.damage, ev);
    }

    fn finish_tick(&mut self, ev: &mut Vec<Event>) {
        self.enemies.retain(|e| e.alive);
        for (i, s) in self.wave_stats.iter_mut().enumerate() {
            if s.started_at.is_some() && s.cleared_at.is_none() && s.pending == 0 && s.alive == 0 {
                s.cleared_at = Some(self.tick);
                ev.push(Event::WaveCleared { wave: i as u16 + 1 });
            }
        }
        if self.outcome == Outcome::Running
            && usize::from(self.wave) == self.wave_stats.len()
            && self.wave_stats.iter().all(|s| s.cleared_at.is_some())
        {
            self.outcome = Outcome::Won;
            ev.push(Event::Won);
        }
    }

    // ---- verification and debugging ----

    /// A 64-bit hash of the entire simulation state (content included), in a fixed order.
    /// Same seed + same commands → same hash every tick, on every machine.
    fn hash(&self, content: &Content) -> u64 {
        let mut h = StableHasher::with_seed(0x5e9_5747e);
        let w32 = |h: &mut StableHasher, v: u32| h.write_u32(v);
        let wi = |h: &mut StableHasher, v: i32| h.write_i64(i64::from(v));
        let wopt = |h: &mut StableHasher, v: Option<u32>| {
            h.write_bool(v.is_some());
            h.write_u32(v.unwrap_or(0));
        };
        h.write_u64(content.hash);
        h.write_u64(self.seed);
        w32(&mut h, self.tick);
        w32(&mut h, u32::from(self.wave));
        wopt(&mut h, self.next_wave_tick);
        wi(&mut h, self.dust);
        wi(&mut h, self.tombstones);
        h.write_u8(self.outcome as u8);
        for r in [self.rng_waves, self.rng_jump, self.rng_drops] {
            let (s, i) = r.to_parts();
            h.write_u64(s);
            h.write_u64(i);
        }
        w32(&mut h, self.next_enemy);
        w32(&mut h, self.next_projectile);
        w32(&mut h, self.next_seq);
        h.write_u64(self.pending.len() as u64);
        for p in &self.pending {
            w32(&mut h, p.tick);
            w32(&mut h, p.seq);
            h.write_u8(p.enemy.0);
            w32(&mut h, u32::from(p.wave));
            match p.at {
                SpawnAt::Ground(c) => {
                    h.write_u8(0);
                    w32(&mut h, u32::from(c.0));
                }
                SpawnAt::Air { entry, dir, hops } => {
                    h.write_u8(1);
                    w32(&mut h, u32::from(entry));
                    wi(&mut h, i32::from(dir));
                    w32(&mut h, u32::from(hops));
                }
            }
        }
        h.write_u64(self.enemies.len() as u64);
        for e in &self.enemies {
            w32(&mut h, e.id);
            h.write_u8(e.kind.0);
            w32(&mut h, u32::from(e.wave));
            wi(&mut h, e.hp);
            wi(&mut h, e.max_hp);
            for v in [e.pos.x, e.pos.y, e.pos.z, e.slow, e.remaining] {
                wi(&mut h, v);
            }
            w32(&mut h, e.slow_until);
            wopt(&mut h, e.attacking.map(|c| u32::from(c.0)));
            h.write_bool(e.alive);
            h.write_i64(e.carry);
            w32(&mut h, e.next_summon);
            match e.motion {
                Motion::Ground { from, to, progress, len, leap } => {
                    h.write_u8(0);
                    w32(&mut h, u32::from(from.0));
                    w32(&mut h, u32::from(to.0));
                    wi(&mut h, progress);
                    wi(&mut h, len);
                    h.write_bool(leap);
                }
                Motion::Air { from, to, progress, len, ring, dir, hops_left, diving } => {
                    h.write_u8(1);
                    for v in [from.x, from.y, from.z, to.x, to.y, to.z, progress, len, i32::from(dir)] {
                        wi(&mut h, v);
                    }
                    w32(&mut h, u32::from(ring));
                    w32(&mut h, u32::from(hops_left));
                    h.write_bool(diving);
                }
            }
        }
        h.write_u64(self.stones.len() as u64);
        for (c, s) in &self.stones {
            w32(&mut h, u32::from(c.0));
            h.write_u8(s.kind.0);
            h.write_u8(s.tier);
            wi(&mut h, s.invested);
            w32(&mut h, s.ready_at);
        }
        h.write_u64(self.walls.len() as u64);
        for (c, w) in &self.walls {
            w32(&mut h, u32::from(c.0));
            wi(&mut h, w.hp);
            wi(&mut h, w.max_hp);
            wi(&mut h, w.cost);
        }
        h.write_u64(self.projectiles.len() as u64);
        for p in &self.projectiles {
            w32(&mut h, p.id);
            h.write_u8(p.stone.0);
            h.write_u8(p.tier);
            for v in [p.origin.x, p.origin.y, p.origin.z, p.pos.x, p.pos.y, p.pos.z, p.aim.x, p.aim.y, p.aim.z] {
                wi(&mut h, v);
            }
            wopt(&mut h, p.target);
            h.write_bool(p.homing);
        }
        for s in &self.wave_stats {
            wopt(&mut h, s.started_at);
            wopt(&mut h, s.cleared_at);
            for v in [s.spawned, s.killed, s.leaked, s.pending, s.alive] {
                w32(&mut h, v);
            }
            wi(&mut h, s.tombstones_lost);
            wi(&mut h, s.dust_from_kills);
        }
        h.finish()
    }

    /// The map with stones (their glyph), walls (`=`) and ground enemies (their glyph, or a
    /// count when several share a cell) drawn in, under a one-line status header.
    fn ascii(&self, content: &Content) -> String {
        let mut count = vec![0u32; content.map.cells().len()];
        let mut glyph = vec![' '; content.map.cells().len()];
        for e in &self.enemies {
            if let Some(c) = e.cell() {
                count[c.index()] += 1;
                glyph[c.index()] = content.enemy(e.kind).glyph;
            }
        }
        let flying = self.enemies.iter().filter(|e| e.cell().is_none()).count();
        let mut out = format!(
            "tick {} ({}s)  wave {}/{}  dust {}  tombstones {}  enemies {} ({} flying)  {:?}\n",
            self.tick,
            self.tick / TICK_HZ,
            self.wave,
            content.waves.len(),
            self.dust,
            self.tombstones,
            self.enemies.len(),
            flying,
            self.outcome
        );
        out.push_str(&content.map.ascii_with(|id| {
            if let Some(s) = self.stones.get(&id) {
                return Some(content.stone(s.kind).glyph);
            }
            if self.walls.contains_key(&id) {
                return Some('=');
            }
            match count[id.index()] {
                0 => None,
                1 => Some(glyph[id.index()]),
                n @ 2..=9 => char::from_digit(n, 10),
                _ => Some('+'),
            }
        }));
        out
    }
}

pub(crate) fn class_of(def: &EnemyDef) -> Class {
    if def.climbs() {
        Class::Climber
    } else {
        Class::Walker
    }
}

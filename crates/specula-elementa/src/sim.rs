//! The simulation: nodes, their state, the acts that change it, and the step.

use crate::config::{Config, Material, MAX_FUEL_S};
use crate::error::ElementaError;
use crate::grid::{box_gap, cell_box, cell_of, cells_in, Grid, MAX_COORD, MAX_NODE_CELLS};
use specula_core::hash::avalanche;
use specula_core::json::Json;
use specula_core::math::Vec3;
use specula_core::rng::Rng;

// The model's constants: the engine's own, chosen for a game that reads and predicts well, not
// taken from a measurement (docs/design/EFFECTUS.md, "Elementa's numbers", says why each).

/// Spreading downwind: the lean grows by this much per m/s of wind along the gap.
pub const K_WIND: f32 = 0.25;
/// Spreading upward: the lean grows by this much for a neighbour straight above (+Y).
pub const K_UP: f32 = 0.5;
/// The least lean: a fire backing into the wind still creeps.
pub const W_MIN: f32 = 0.1;
/// Heat it takes to dry a thing from wet 1 to 0 (wetness takes heat first).
pub const LATENT: f32 = 0.5;
/// Above this wetness nothing catches.
pub const WET_BLOCK: f32 = 0.05;
/// At or above this wetness a burning thing goes out.
pub const DOUSE: f32 = 0.5;
/// Wetness a second a thing dries at ambient heat.
pub const DRY_BASE: f32 = 1.0 / 600.0;
/// More wetness a second it dries per unit of heat above ambient 0.
pub const DRY_HOT: f32 = 0.05;
/// Wetness a second a burning thing dries itself.
pub const BURN_DRY: f32 = 0.1;
/// The fraction of the gap to its flame a burning thing closes a second.
pub const FLAME_RATE: f32 = 2.0;
/// A frozen thing thaws this much above its freezing heat (so a thing at the line does not flicker).
pub const THAW_MARGIN: f32 = 0.1;
/// Heat an ember gives the thing it lands on.
pub const EMBER_HEAT: f32 = 0.25;
/// How far past the reach an ember can fly in a 4 m/s wind (further in more, up to 4 times).
pub const EMBER_M: f32 = 3.0;
/// Wind it takes to carry an ember, m/s.
pub const EMBER_WIND: f32 = 1.0;
/// The strongest wind, m/s.
pub const MAX_WIND: f32 = 64.0;
/// A box's half size on an axis, at most, metres.
pub const MAX_HALF: f32 = 10_000.0;
/// Events of one kind waiting to be taken, named; the rest are counted.
pub const MAX_EVENTS_PER_KIND: usize = 64;
/// Acts (ignite, chill, wet) between two ticks.
pub const MAX_ACTS_PER_TICK: u32 = 64;
/// An act's sphere, at most, metres.
pub const MAX_ACT_RADIUS: f32 = 64.0;
/// The engine's tick rate, at most.
pub const MAX_TICK_HZ: u32 = 1000;
/// The latest tick a state may be at (a save past it is refused): 2^62, 146 million years at
/// 1,000 ticks a second, so the clock never wraps.
pub const MAX_TICK: u64 = 1 << 62;
/// `Node::ready_at` of a node not waiting to catch.
pub const NOT_READY: u64 = u64::MAX;

/// Where a node stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Intact = 0,
    Burning = 1,
    Burnt = 2,
    Frozen = 3,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Intact => "intact",
            Phase::Burning => "burning",
            Phase::Burnt => "burnt",
            Phase::Frozen => "frozen",
        }
    }

    pub fn from_u8(v: u8) -> Option<Phase> {
        [Phase::Intact, Phase::Burning, Phase::Burnt, Phase::Frozen].get(usize::from(v)).copied()
    }
}

/// One thing Elementa tracks: an entity's id, what it is made of, its box, and its state.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: u64,
    pub material: u16,
    pub center: Vec3,
    pub half: Vec3,
    /// −1 frozen solid .. 0 ambient .. 1 white hot.
    pub heat: f32,
    /// Seconds of burning left.
    pub fuel: f32,
    /// 0 dry .. 1 soaked.
    pub wet: f32,
    pub phase: Phase,
    /// The tick its phase began.
    pub since: u64,
    /// For a burning node: spread steps of heat it has not yet given (the work budget passed it
    /// over, or it is part-way through a query the budget cut); given all at once as its
    /// neighbours are next reached. 0 for any other node.
    pub owed: u32,
    /// For a node ready to catch but held by the burning limit: the tick it became ready
    /// ([`NOT_READY`] otherwise). The longest waiting catch first.
    pub ready_at: u64,
}

/// Where the next spread step resumes giving heat after a step the work budget cut: the
/// burning node, and within its neighbour query the cell (counted in x, y, z order over the
/// query's cells) and the entry within that cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub id: u64,
    pub cell: u32,
    pub entry: u32,
}

/// A node as the caller gives it. Unset state starts from the material.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDesc {
    pub id: u64,
    pub material: u16,
    pub center: Vec3,
    pub half: Vec3,
    pub fuel_s: Option<f32>,
    pub heat: Option<f32>,
    pub wet: Option<f32>,
}

impl NodeDesc {
    pub fn new(id: u64, material: u16, center: Vec3, half: Vec3) -> NodeDesc {
        NodeDesc { id, material, center, half, fuel_s: None, heat: None, wet: None }
    }

    pub fn fuel(mut self, s: f32) -> NodeDesc {
        self.fuel_s = Some(s);
        self
    }

    pub fn heat(mut self, h: f32) -> NodeDesc {
        self.heat = Some(h);
        self
    }

    pub fn wet(mut self, w: f32) -> NodeDesc {
        self.wet = Some(w);
        self
    }
}

/// What a batch of nodes came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddReport {
    pub added: u32,
    /// Left out because the node limit was reached (the highest ids of the batch).
    pub left_out: u32,
    /// The node limit.
    pub limit: u32,
}

/// What an act does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActKind {
    /// Raise heat to `amount` (0..1]; wetness takes it first.
    Ignite,
    /// Take `amount` (0..2] of heat away.
    Chill,
    /// Add `amount` (0..1] of wetness.
    Wet,
}

impl ActKind {
    pub fn name(self) -> &'static str {
        match self {
            ActKind::Ignite => "ignite",
            ActKind::Chill => "chill",
            ActKind::Wet => "wet",
        }
    }
}

/// Which nodes an act reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Node(u64),
    /// Every node whose box comes within `radius` of `at`.
    Sphere { at: Vec3, radius: f32 },
}

/// A one-shot act, applied at once; what it starts shows at the next spread step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Act {
    pub kind: ActKind,
    pub target: Target,
    pub amount: f32,
}

/// What an act reached and what it cost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActReport {
    pub nodes: u32,
    /// Cell lookups and entries or nodes looked at: at most twice the node count, plus one.
    pub work: u32,
    /// The grid's rebuild ([`crate::grid::rebuild_ops`]) when nodes were added, removed or
    /// moved since the last one; else 0.
    pub grid_ops: u64,
}

/// What happened to a node, told to the rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    Ignited = 0,
    Burnt = 1,
    Extinguished = 2,
    Frozen = 3,
    Thawed = 4,
}

pub const EVENT_KINDS: [EventKind; 5] =
    [EventKind::Ignited, EventKind::Burnt, EventKind::Extinguished, EventKind::Frozen, EventKind::Thawed];

impl EventKind {
    /// The event's name for modules (`element.ignited`).
    pub fn name(self) -> &'static str {
        match self {
            EventKind::Ignited => "element.ignited",
            EventKind::Burnt => "element.burnt",
            EventKind::Extinguished => "element.extinguished",
            EventKind::Frozen => "element.frozen",
            EventKind::Thawed => "element.thawed",
        }
    }
}

/// What put a fire out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Cold = 1,
    Wet = 2,
}

impl Cause {
    pub fn name(self) -> &'static str {
        match self {
            Cause::Cold => "cold",
            Cause::Wet => "wet",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub tick: u64,
    pub kind: EventKind,
    pub id: u64,
    pub at: Vec3,
    /// For `Extinguished`: why.
    pub by: Option<Cause>,
}

impl Event {
    pub fn to_json(&self) -> Json {
        let mut f = vec![
            ("event", Json::from(self.kind.name())),
            ("tick", Json::from(self.tick)),
            ("id", Json::from(self.id)),
            ("at", Json::arr([Json::from(self.at.x), Json::from(self.at.y), Json::from(self.at.z)])),
        ];
        if let Some(by) = self.by {
            f.push(("by", Json::from(by.name())));
        }
        Json::obj(f)
    }
}

/// Events waiting to be taken: in tick order; within a step by node id, then kind. At most
/// [`MAX_EVENTS_PER_KIND`] of a kind are named; the rest are counted in `unnamed`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Events {
    pub list: Vec<Event>,
    /// Per [`EventKind`] (by its number): events past the cap.
    pub unnamed: [u32; 5],
}

impl Events {
    fn push(&mut self, e: Event) {
        let k = e.kind as usize;
        if self.list.iter().filter(|x| x.kind == e.kind).count() < MAX_EVENTS_PER_KIND {
            self.list.push(e);
        } else {
            self.unnamed[k] = self.unnamed[k].saturating_add(1);
        }
    }

    /// Named and counted, per kind.
    pub fn count(&self, kind: EventKind) -> u64 {
        self.list.iter().filter(|e| e.kind == kind).count() as u64 + u64::from(self.unnamed[kind as usize])
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && self.unnamed.iter().all(|&u| u == 0)
    }
}

/// What the last tick did and cost. Work is counted in cell lookups plus neighbour checks
/// (each a few comparisons and, past the box test, one square root); node updates are one each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepReport {
    pub tick: u64,
    /// A spread step ran this tick (else nothing moved).
    pub spread: bool,
    pub nodes: u32,
    pub burning: u32,
    /// Burning nodes whose neighbours were heated this step.
    pub processed: u32,
    /// Burning nodes carried to the next step because the work budget was spent.
    pub deferred: u32,
    /// Burning nodes whose query the budget cut short.
    pub truncated: u32,
    /// Nodes ready to catch that waited for the burning limit.
    pub waiting: u32,
    /// Nodes ready to catch, put in order (longest waiting first) to choose which catch.
    pub ordered: u32,
    pub embers: u32,
    pub lookups: u32,
    pub checks: u32,
    /// `lookups + checks`: never above the work limit.
    pub work: u32,
    /// One per node (heat, phase, relaxation).
    pub node_updates: u32,
    pub grid_rebuilt: bool,
    pub grid_entries: u32,
    /// The grid's rebuild ([`crate::grid::rebuild_ops`]); 0 when nothing was added, removed or moved.
    pub grid_ops: u64,
    pub events: u32,
}

impl StepReport {
    /// Node updates, the ordering of ready nodes, the work and the grid's rebuild: never above
    /// [`crate::config::WorstCase::ops_per_step`], plus [`crate::config::WorstCase::rebuild_ops`]
    /// in a step after nodes were added, removed or moved.
    pub fn ops(&self) -> u64 {
        u64::from(self.node_updates) + u64::from(self.ordered) + u64::from(self.work) + self.grid_ops
    }
}

/// Elementa's whole state for one world.
#[derive(Clone, Debug)]
pub struct Elementa {
    pub(crate) config: Config,
    pub(crate) tick_hz: u32,
    pub(crate) every: u32,
    pub(crate) tick: u64,
    pub(crate) rng: Rng,
    /// Sorted by id.
    pub(crate) nodes: Vec<Node>,
    pub(crate) wind: Vec3,
    pub(crate) ambient: f32,
    /// Where the next spread step starts giving heat (after a step cut by the work budget).
    pub(crate) cursor: Cursor,
    /// Nodes refused or left out for the node limit, all told.
    pub(crate) refused: u64,
    pub(crate) acts: u32,
    pub(crate) events: Events,
    grid: Grid,
    grid_dirty: bool,
    gain: Vec<f32>,
    pub(crate) report: StepReport,
}

/// How much a neighbour's direction leans the heat it is given: downwind and upward more.
pub fn lean(wind: Vec3, from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    let len = d.length();
    let u = if len > 1.0e-6 { d * (1.0 / len) } else { Vec3::ZERO };
    (1.0 + K_WIND * wind.dot(u) + K_UP * u.y).max(W_MIN)
}

/// Heat a second a burning thing gives a neighbour `gap` metres away: falling off linearly to 0
/// at the reach, times the lean.
pub fn heat_rate(heat_out: f32, gap: f32, reach: f32, lean: f32) -> f32 {
    let f = if reach > 0.0 { 1.0 - gap / reach } else { 1.0 };
    heat_out * f * lean
}

/// Takes `q` of heat into a node: wetness takes it first (except in ice, whose water is held).
pub(crate) fn absorb(n: &mut Node, mut q: f32) {
    if q <= 0.0 {
        n.heat = (n.heat + q).max(-1.0);
        return;
    }
    if n.phase != Phase::Frozen && n.wet > 0.0 {
        let used = q.min(n.wet * LATENT);
        n.wet = (n.wet - used / LATENT).max(0.0);
        q -= used;
    }
    n.heat = (n.heat + q).min(1.0);
}

/// One step of time for a node's heat, fuel and wetness.
pub(crate) fn relax(n: &mut Node, m: &Material, ambient: f32, dt: f32) {
    if n.phase == Phase::Burning {
        n.fuel = (n.fuel - dt).max(0.0);
        n.heat += (m.flame - n.heat) * (FLAME_RATE * dt).min(1.0);
        n.wet = (n.wet - BURN_DRY * dt).max(m.quench).min(1.0);
    } else {
        n.heat += (ambient - n.heat) * (m.cool * dt).min(1.0);
        n.wet = (n.wet - dt * (DRY_BASE + DRY_HOT * n.heat.max(0.0))).max(m.quench).min(1.0);
    }
}

/// Whether an intact node is ready to catch.
pub(crate) fn ready(n: &Node, m: &Material) -> bool {
    match m.ignite {
        Some(ig) => n.phase == Phase::Intact && n.fuel > 0.0 && n.heat >= ig && n.wet <= WET_BLOCK,
        None => false,
    }
}

fn finite3(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

/// The checks a node's box must pass, from a call or a save.
pub(crate) fn check_box(config: &Config, id: u64, center: Vec3, half: Vec3) -> Result<(), ElementaError> {
    let lim = f64::from(MAX_COORD);
    for (axis, c) in [("x", center.x), ("y", center.y), ("z", center.z)] {
        if !c.is_finite() || c.abs() > MAX_COORD {
            return Err(ElementaError::range(&format!("node {id} center.{axis}"), f64::from(c), -lim, lim).with("id", id));
        }
    }
    for (axis, h) in [("x", half.x), ("y", half.y), ("z", half.z)] {
        if !h.is_finite() || !(0.0..=MAX_HALF).contains(&h) {
            return Err(ElementaError::range(&format!("node {id} half.{axis}"), f64::from(h), 0.0, f64::from(MAX_HALF)).with("id", id));
        }
    }
    let (lo, hi) = cell_box(center - half, center + half, config.cell_m);
    let cells = cells_in(lo, hi);
    if cells > u64::from(MAX_NODE_CELLS) {
        return Err(ElementaError::new(
            "too_large",
            format!(
                "node {id} is {} x {} x {} m and covers {cells} cells of {} m; at most {MAX_NODE_CELLS}: give it as several nodes, or a larger cell_m",
                half.x * 2.0,
                half.y * 2.0,
                half.z * 2.0,
                config.cell_m
            ),
        )
        .with("id", id)
        .with("cells", cells)
        .with("max", MAX_NODE_CELLS)
        .with("cell_m", config.cell_m));
    }
    Ok(())
}

impl Elementa {
    /// A new, empty simulation. `tick_hz` is the engine's tick rate: a whole multiple of the
    /// config's `spread_hz`. `seed` names the run's stream (embers).
    pub fn new(config: Config, tick_hz: u32, seed: u64) -> Result<Elementa, ElementaError> {
        config.validate()?;
        let every = check_rate(&config, tick_hz)?;
        Ok(Elementa {
            config,
            tick_hz,
            every,
            tick: 0,
            rng: Rng::named(seed, "elementa"),
            nodes: Vec::new(),
            wind: Vec3::ZERO,
            ambient: 0.0,
            cursor: Cursor::default(),
            refused: 0,
            acts: 0,
            events: Events::default(),
            grid: Grid::default(),
            grid_dirty: true,
            gain: Vec::new(),
            report: StepReport::default(),
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn tick_hz(&self) -> u32 {
        self.tick_hz
    }

    /// Ticks between two spread steps.
    pub fn ticks_per_step(&self) -> u32 {
        self.every
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn node(&self, id: u64) -> Option<&Node> {
        self.nodes.binary_search_by_key(&id, |n| n.id).ok().map(|i| &self.nodes[i])
    }

    pub fn wind(&self) -> Vec3 {
        self.wind
    }

    pub fn ambient(&self) -> f32 {
        self.ambient
    }

    /// Nodes refused or left out for the node limit, all told.
    pub fn refused(&self) -> u64 {
        self.refused
    }

    pub fn burning(&self) -> u32 {
        self.nodes.iter().filter(|n| n.phase == Phase::Burning).count() as u32
    }

    pub fn last_report(&self) -> &StepReport {
        &self.report
    }

    /// Events since they were last taken (the engine takes them each tick, for the next).
    pub fn take_events(&mut self) -> Events {
        std::mem::take(&mut self.events)
    }

    pub fn events(&self) -> &Events {
        &self.events
    }

    fn make(&self, d: &NodeDesc) -> Result<Node, ElementaError> {
        let m = self.config.materials.get(usize::from(d.material)).ok_or_else(|| {
            ElementaError::new("unknown_material", format!("node {} names material {}; there are {}", d.id, d.material, self.config.materials.len()))
                .with("id", d.id)
                .with("material", u32::from(d.material))
        })?;
        check_box(&self.config, d.id, d.center, d.half)?;
        let field = |name: &str, v: Option<f32>, min: f32, max: f32| -> Result<Option<f32>, ElementaError> {
            match v {
                Some(x) if !x.is_finite() || x < min || x > max => {
                    Err(ElementaError::range(&format!("node {} {name}", d.id), f64::from(x), f64::from(min), f64::from(max)).with("id", d.id))
                }
                _ => Ok(v),
            }
        };
        let fuel = field("fuel_s", d.fuel_s, 0.0, MAX_FUEL_S)?;
        let heat = field("heat", d.heat, -1.0, 1.0)?.unwrap_or(m.start_heat);
        let wet = field("wet", d.wet, 0.0, 1.0)?.unwrap_or(0.0).max(m.quench);
        let fuel = if m.ignite.is_some() { fuel.unwrap_or(m.burn_s) } else { 0.0 };
        let phase = match m.freeze {
            Some(f) if heat <= f => Phase::Frozen,
            _ => Phase::Intact,
        };
        Ok(Node { id: d.id, material: d.material, center: d.center, half: d.half, heat, fuel, wet, phase, since: self.tick, owed: 0, ready_at: NOT_READY })
    }

    /// Tracks one node. Refused, with the number, when the node limit is reached.
    pub fn add(&mut self, d: NodeDesc) -> Result<(), ElementaError> {
        let node = self.make(&d)?;
        let at = match self.nodes.binary_search_by_key(&d.id, |n| n.id) {
            Ok(_) => return Err(ElementaError::new("duplicate", format!("node {} is already tracked", d.id)).with("id", d.id)),
            Err(at) => at,
        };
        let max = self.config.limits.nodes;
        if self.nodes.len() >= max as usize {
            self.refused = self.refused.saturating_add(1);
            return Err(ElementaError::limit(
                "nodes",
                u64::from(max),
                self.nodes.len() as u64 + 1,
                format!("{max} nodes are tracked already, the limit; node {} is left out", d.id),
            )
            .with("id", d.id));
        }
        self.nodes.insert(at, node);
        self.grid_dirty = true;
        Ok(())
    }

    /// Tracks many nodes at once (one sort, not one insertion each). Every node is checked before
    /// any is added; past the node limit the highest ids are left out and counted.
    pub fn add_many(&mut self, descs: &[NodeDesc]) -> Result<AddReport, ElementaError> {
        let mut made = descs.iter().map(|d| self.make(d)).collect::<Result<Vec<Node>, ElementaError>>()?;
        made.sort_by_key(|n| n.id);
        for (i, n) in made.iter().enumerate() {
            if (i > 0 && made[i - 1].id == n.id) || self.node(n.id).is_some() {
                return Err(ElementaError::new("duplicate", format!("node {} is given twice or already tracked", n.id)).with("id", n.id));
            }
        }
        let max = self.config.limits.nodes as usize;
        let room = max.saturating_sub(self.nodes.len());
        let left_out = made.len().saturating_sub(room);
        made.truncate(room);
        self.refused = self.refused.saturating_add(left_out as u64);
        let added = made.len() as u32;
        self.nodes.extend(made);
        self.nodes.sort_by_key(|n| n.id);
        self.grid_dirty = true;
        Ok(AddReport { added, left_out: left_out as u32, limit: max as u32 })
    }

    /// Stops tracking a node; `false` if it was not tracked.
    pub fn remove(&mut self, id: u64) -> bool {
        match self.nodes.binary_search_by_key(&id, |n| n.id) {
            Ok(i) => {
                if self.nodes.remove(i).phase == Phase::Burning {
                    self.report.burning = self.report.burning.saturating_sub(1);
                }
                self.grid_dirty = true;
                true
            }
            Err(_) => false,
        }
    }

    /// A node's entity moved or changed size.
    pub fn move_node(&mut self, id: u64, center: Vec3, half: Vec3) -> Result<(), ElementaError> {
        check_box(&self.config, id, center, half)?;
        let i = self.index(id)?;
        self.nodes[i].center = center;
        self.nodes[i].half = half;
        self.grid_dirty = true;
        Ok(())
    }

    fn index(&self, id: u64) -> Result<usize, ElementaError> {
        self.nodes
            .binary_search_by_key(&id, |n| n.id)
            .map_err(|_| ElementaError::new("unknown_node", format!("node {id} is not tracked")).with("id", id))
    }

    /// The world's wind, m/s (`world_env.wind`: strength 1 is 4 m/s).
    pub fn set_wind(&mut self, v: Vec3) -> Result<(), ElementaError> {
        if !finite3(v) || v.length() > MAX_WIND {
            return Err(ElementaError::range("wind", f64::from(if finite3(v) { v.length() } else { f32::INFINITY }), 0.0, f64::from(MAX_WIND)));
        }
        self.wind = v;
        Ok(())
    }

    /// The heat things return to (−1..1): below 0 a winter, where water freezes.
    pub fn set_ambient(&mut self, h: f32) -> Result<(), ElementaError> {
        if !h.is_finite() || !(-1.0..=1.0).contains(&h) {
            return Err(ElementaError::range("ambient", f64::from(h), -1.0, 1.0));
        }
        self.ambient = h;
        Ok(())
    }

    /// Rebuilds the grid if nodes were added, removed or moved; its cost in ops (0 if not).
    fn rebuild(&mut self) -> u64 {
        if self.grid_dirty {
            self.grid_dirty = false;
            return self.grid.rebuild(&self.nodes, self.config.cell_m);
        }
        0
    }

    /// What rebuilding the grid would cost now (0 when it is up to date): said before a run.
    pub fn pending_rebuild_ops(&self) -> u64 {
        if self.grid_dirty {
            crate::grid::rebuild_ops(crate::grid::entries_for(&self.nodes, self.config.cell_m))
        } else {
            0
        }
    }

    /// Forgets the tick's acts (a copy made for a prediction starts with none).
    pub(crate) fn clear_acts(&mut self) {
        self.acts = 0;
    }

    /// Applies an act at once. At most [`MAX_ACTS_PER_TICK`] between two ticks; its sphere at
    /// most [`MAX_ACT_RADIUS`]. Its work is at most twice the node count plus one: the sphere's
    /// cells are looked up first (only when there are no more of them than nodes), and when
    /// they hold more entries than there are nodes every node is looked at once instead. A
    /// grid rebuild due is counted apart (`grid_ops`).
    pub fn act(&mut self, a: Act) -> Result<ActReport, ElementaError> {
        if self.acts >= MAX_ACTS_PER_TICK {
            return Err(ElementaError::limit(
                "acts",
                u64::from(MAX_ACTS_PER_TICK),
                u64::from(self.acts) + 1,
                format!("{MAX_ACTS_PER_TICK} acts in one tick already"),
            ));
        }
        let (lo, hi) = match a.kind {
            ActKind::Ignite => (0.0, 1.0),
            ActKind::Chill => (0.0, 2.0),
            ActKind::Wet => (0.0, 1.0),
        };
        if !a.amount.is_finite() || a.amount <= lo || a.amount > hi {
            return Err(ElementaError::range(&format!("{}.amount", a.kind.name()), f64::from(a.amount), f64::from(lo), f64::from(hi)));
        }
        let mut report = ActReport::default();
        let mut hit: Vec<usize> = Vec::new();
        match a.target {
            Target::Node(id) => {
                hit.push(self.index(id)?);
                report.work = 1;
            }
            Target::Sphere { at, radius } => {
                if !finite3(at) || at.x.abs() > MAX_COORD || at.y.abs() > MAX_COORD || at.z.abs() > MAX_COORD {
                    return Err(ElementaError::range(&format!("{}.at", a.kind.name()), f64::from(at.length()), 0.0, f64::from(MAX_COORD)));
                }
                if !radius.is_finite() || !(0.0..=MAX_ACT_RADIUS).contains(&radius) {
                    return Err(ElementaError::range(&format!("{}.radius", a.kind.name()), f64::from(radius), 0.0, f64::from(MAX_ACT_RADIUS)));
                }
                report.grid_ops = self.rebuild();
                let r = Vec3::new(radius, radius, radius);
                let (clo, chi) = cell_box(at - r, at + r, self.config.cell_m);
                let within = |n: &Node| box_gap(at, Vec3::ZERO, n.center, n.half) <= radius;
                let n = self.nodes.len() as u64;
                let cells = cells_in(clo, chi);
                let mut ranges: Vec<([i32; 3], usize, usize)> = Vec::new();
                let mut entries = 0u64;
                if cells <= n {
                    for x in clo[0]..=chi[0] {
                        for y in clo[1]..=chi[1] {
                            for z in clo[2]..=chi[2] {
                                report.work += 1;
                                let (a, b) = self.grid.range([x, y, z]);
                                entries += (b - a) as u64;
                                ranges.push(([x, y, z], a, b));
                            }
                        }
                    }
                }
                if cells > n || cells + entries > n {
                    // More cells, or more entries in them, than there are nodes: every node once.
                    for (i, node) in self.nodes.iter().enumerate() {
                        report.work += 1;
                        if within(node) {
                            hit.push(i);
                        }
                    }
                } else {
                    let all = self.grid.entries();
                    for (c, a, b) in ranges {
                        for e in &all[a..b] {
                            report.work += 1;
                            let node = &self.nodes[e.node as usize];
                            if first_shared_cell(node, clo, self.config.cell_m) == c && within(node) {
                                hit.push(e.node as usize);
                            }
                        }
                    }
                    hit.sort_unstable();
                }
            }
        }
        for &i in &hit {
            let n = &mut self.nodes[i];
            match a.kind {
                ActKind::Ignite => {
                    if matches!(n.phase, Phase::Intact | Phase::Frozen) && a.amount > n.heat {
                        absorb(n, a.amount - n.heat);
                    }
                }
                ActKind::Chill => n.heat = (n.heat - a.amount).max(-1.0),
                ActKind::Wet => n.wet = (n.wet + a.amount).min(1.0),
            }
        }
        self.acts += 1;
        report.nodes = hit.len() as u32;
        Ok(report)
    }

    /// Advances one tick. Every `ticks_per_step` ticks a spread step runs: burning things heat
    /// their neighbours, phases change, everything relaxes, and events are queued.
    pub fn step(&mut self) -> &StepReport {
        self.tick = self.tick.saturating_add(1).min(MAX_TICK);
        self.acts = 0;
        if !self.tick.is_multiple_of(u64::from(self.every)) {
            // Nothing moves between spread steps: the burning count is the last step's.
            self.report = StepReport { tick: self.tick, nodes: self.nodes.len() as u32, burning: self.report.burning, ..StepReport::default() };
            return &self.report;
        }
        self.spread();
        &self.report
    }

    fn spread(&mut self) {
        let dt = 1.0 / self.config.spread_hz as f32;
        let mut rep = StepReport { tick: self.tick, spread: true, nodes: self.nodes.len() as u32, ..StepReport::default() };
        rep.grid_ops = self.rebuild();
        rep.grid_rebuilt = rep.grid_ops > 0;
        rep.grid_entries = self.grid.len() as u32;
        let n = self.nodes.len();
        self.gain.clear();
        self.gain.resize(n, 0.0);

        // Heat given: burning nodes in id order from the cursor, round to the start, within the
        // work budget. Each burning node is owed one step of heat now; it gives all it is owed
        // when its query is done, so a node the budget passes over gives its heat late, not
        // never. A query the budget cuts resumes where it stopped. Gains are summed first and
        // applied after, so no node's place in the order changes another's result.
        let mut spend = Spend { used: 0, budget: u64::from(self.config.limits.work) };
        let resume = self.cursor;
        let start = self.nodes.partition_point(|x| x.id < resume.id);
        let mut next_cursor = None;
        for k in 0..n {
            let i = (start + k) % n;
            if self.nodes[i].phase != Phase::Burning {
                continue;
            }
            self.nodes[i].owed = self.nodes[i].owed.saturating_add(1);
            if spend.used >= spend.budget {
                next_cursor.get_or_insert(Cursor { id: self.nodes[i].id, cell: 0, entry: 0 });
                rep.deferred += 1;
                continue;
            }
            rep.processed += 1;
            let from = if self.nodes[i].id == resume.id { (resume.cell, resume.entry) } else { (0, 0) };
            let factor = self.nodes[i].owed as f32;
            let ember = if from == (0, 0) { self.ember(i, dt * factor) } else { None };
            let q = Query { nodes: &self.nodes, grid: &self.grid, config: &self.config, wind: self.wind };
            if let Some(at) = ember {
                rep.embers += 1;
                q.ember(at, &mut self.gain, &mut spend, &mut rep);
            }
            match q.heat(i, dt * factor, from, &mut self.gain, &mut spend, &mut rep) {
                Some((cell, entry)) => {
                    rep.truncated += 1;
                    next_cursor.get_or_insert(Cursor { id: self.nodes[i].id, cell, entry });
                }
                None => self.nodes[i].owed = 0,
            }
        }
        self.cursor = next_cursor.unwrap_or_default();
        rep.work = spend.used as u32;

        // Phases: first what ends (out, burnt, frozen, thawed), in id order; then what catches.
        let tick = self.tick;
        let mut events: Vec<Event> = Vec::new();
        let ev = |kind, n: &Node, by| Event { tick, kind, id: n.id, at: n.center, by };
        for (j, node) in self.nodes.iter_mut().enumerate() {
            let m = &self.config.materials[usize::from(node.material)];
            rep.node_updates += 1;
            if self.gain[j] > 0.0 {
                absorb(node, self.gain[j]);
            }
            match node.phase {
                Phase::Burning => {
                    let ig = m.ignite.unwrap_or(0.0);
                    let out = if node.wet >= DOUSE {
                        Some(Cause::Wet)
                    } else if node.heat < ig {
                        Some(Cause::Cold)
                    } else {
                        None
                    };
                    if let Some(by) = out {
                        node.phase = Phase::Intact;
                        node.since = tick;
                        node.owed = 0;
                        events.push(ev(EventKind::Extinguished, node, Some(by)));
                    } else if node.fuel <= 0.0 {
                        node.phase = Phase::Burnt;
                        node.since = tick;
                        node.owed = 0;
                        events.push(ev(EventKind::Burnt, node, None));
                    }
                }
                Phase::Intact => {
                    if let Some(f) = m.freeze {
                        if node.heat <= f {
                            node.phase = Phase::Frozen;
                            node.since = tick;
                            events.push(ev(EventKind::Frozen, node, None));
                        }
                    }
                }
                Phase::Frozen => {
                    let f = m.freeze.unwrap_or(-1.0);
                    if node.heat > f + THAW_MARGIN {
                        node.phase = Phase::Intact;
                        node.since = tick;
                        events.push(ev(EventKind::Thawed, node, None));
                    }
                }
                Phase::Burnt => {}
            }
        }
        // What catches: every ready node, longest waiting first, then hottest, then in an order
        // drawn afresh each step from the ids (a hash of id and tick), never by id itself: under
        // the burning limit the fire then grows where it was ready first, whichever way the
        // entities' ids run.
        let burning = self.burning();
        let limit = self.config.limits.burning;
        let mut queue: Vec<(u64, f32, u64, usize)> = Vec::new();
        for (j, node) in self.nodes.iter_mut().enumerate() {
            let m = &self.config.materials[usize::from(node.material)];
            if ready(node, m) {
                if node.ready_at == NOT_READY {
                    node.ready_at = tick;
                }
                queue.push((node.ready_at, node.heat, avalanche(node.id ^ tick.wrapping_mul(0x9e37_79b9_7f4a_7c15)), j));
            } else {
                node.ready_at = NOT_READY;
            }
        }
        rep.ordered = queue.len() as u32;
        queue.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
        let room = limit.saturating_sub(burning) as usize;
        for &(_, _, _, j) in queue.iter().take(room) {
            let node = &mut self.nodes[j];
            node.phase = Phase::Burning;
            node.since = tick;
            node.ready_at = NOT_READY;
            node.owed = 0;
            events.push(ev(EventKind::Ignited, node, None));
        }
        rep.waiting = queue.len().saturating_sub(room) as u32;
        let burning = burning + queue.len().min(room) as u32;
        for node in self.nodes.iter_mut() {
            let m = &self.config.materials[usize::from(node.material)];
            relax(node, m, self.ambient, dt);
        }
        events.sort_by_key(|e| (e.id, e.kind));
        rep.events = events.len() as u32;
        for e in events {
            self.events.push(e);
        }
        rep.burning = burning;
        self.report = rep;
    }

    /// One draw of the stream for a burning node that can throw embers in this wind: where its
    /// ember lands, if it throws one.
    fn ember(&mut self, i: usize, dt: f32) -> Option<Vec3> {
        let n = &self.nodes[i];
        let m = &self.config.materials[usize::from(n.material)];
        let speed = self.wind.length();
        if m.embers <= 0.0 || speed < EMBER_WIND {
            return None;
        }
        let draw = self.rng.next_u64();
        let chance = (m.embers * dt * 16_777_216.0) as u32;
        if ((draw as u32) >> 8) >= chance {
            return None;
        }
        let frac = (draw >> 40) as f32 * (1.0 / 16_777_216.0);
        let dir = self.wind * (1.0 / speed);
        let dist = self.config.reach_m + frac * EMBER_M * (speed * 0.25).min(4.0);
        let out = n.half.x.max(n.half.y).max(n.half.z);
        Some(n.center + dir * (out + dist))
    }

    /// A summary for tools (`elementa_info`), within a few hundred bytes.
    pub fn info(&self) -> Json {
        let count = |p: Phase| self.nodes.iter().filter(|n| n.phase == p).count();
        Json::obj([
            ("tick", Json::from(self.tick)),
            ("nodes", Json::from(self.nodes.len())),
            ("burning", Json::from(count(Phase::Burning))),
            ("burnt", Json::from(count(Phase::Burnt))),
            ("frozen", Json::from(count(Phase::Frozen))),
            ("refused", Json::from(self.refused)),
            ("wind", Json::arr([Json::from(self.wind.x), Json::from(self.wind.y), Json::from(self.wind.z)])),
            ("ambient", Json::from(self.ambient)),
            ("hash", Json::from(format!("{:016x}", self.hash()))),
            ("config", self.config.to_json()),
        ])
    }
}

fn check_rate(config: &Config, tick_hz: u32) -> Result<u32, ElementaError> {
    if tick_hz == 0 || tick_hz > MAX_TICK_HZ || !tick_hz.is_multiple_of(config.spread_hz) {
        return Err(ElementaError::new(
            "bad_config",
            format!("the tick rate {tick_hz} must be 1 to {MAX_TICK_HZ} and a whole multiple of spread_hz {}", config.spread_hz),
        )
        .with("tick_hz", tick_hz)
        .with("spread_hz", config.spread_hz));
    }
    Ok(tick_hz / config.spread_hz)
}

/// The cell where a node and a query box first share: where the node is counted.
fn first_shared_cell(n: &Node, query_lo: [i32; 3], cell: f32) -> [i32; 3] {
    let lo = n.center - n.half;
    [cell_of(lo.x, cell).max(query_lo[0]), cell_of(lo.y, cell).max(query_lo[1]), cell_of(lo.z, cell).max(query_lo[2])]
}

struct Query<'a> {
    nodes: &'a [Node],
    grid: &'a Grid,
    config: &'a Config,
    wind: Vec3,
}

/// The work spent in a step against its budget.
struct Spend {
    used: u64,
    budget: u64,
}

impl Spend {
    /// One unit of work, if the budget has it.
    fn take(&mut self) -> bool {
        if self.used >= self.budget {
            return false;
        }
        self.used += 1;
        true
    }
}

impl Query<'_> {
    /// Heats the neighbours of burning node `i`, starting at `from` (a cell, counted in x, y, z
    /// order over the query's cells, and an entry in it). `Some(where to resume)` when the budget
    /// cut it short; `None` when every cell was looked at.
    fn heat(&self, i: usize, dt: f32, from: (u32, u32), gain: &mut [f32], spend: &mut Spend, rep: &mut StepReport) -> Option<(u32, u32)> {
        let a = &self.nodes[i];
        let m = &self.config.materials[usize::from(a.material)];
        if m.heat_out <= 0.0 {
            return None;
        }
        let reach = self.config.reach_m;
        let r = Vec3::new(reach, reach, reach);
        let (lo, hi) = cell_box(a.center - a.half - r, a.center + a.half + r, self.config.cell_m);
        let mut k = 0u32;
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    let this = k;
                    k += 1;
                    if this < from.0 {
                        continue;
                    }
                    if !spend.take() {
                        return Some((this, if this == from.0 { from.1 } else { 0 }));
                    }
                    rep.lookups += 1;
                    let entries = self.grid.cell([x, y, z]);
                    let first = if this == from.0 { (from.1 as usize).min(entries.len()) } else { 0 };
                    for (idx, e) in entries.iter().enumerate().skip(first) {
                        if !spend.take() {
                            return Some((this, idx as u32));
                        }
                        rep.checks += 1;
                        let j = e.node as usize;
                        if j == i {
                            continue;
                        }
                        let b = &self.nodes[j];
                        if matches!(b.phase, Phase::Burning | Phase::Burnt) || first_shared_cell(b, lo, self.config.cell_m) != [x, y, z] {
                            continue;
                        }
                        let gap = box_gap(a.center, a.half, b.center, b.half);
                        if gap > reach {
                            continue;
                        }
                        gain[j] += heat_rate(m.heat_out, gap, reach, lean(self.wind, a.center, b.center)) * dt;
                    }
                }
            }
        }
        None
    }

    /// An ember lands at `at`: the things whose boxes hold that point are given its heat.
    fn ember(&self, at: Vec3, gain: &mut [f32], spend: &mut Spend, rep: &mut StepReport) {
        let c = self.config.cell_m;
        let lim = MAX_COORD + MAX_HALF * 2.0;
        if at.x.abs() > lim || at.y.abs() > lim || at.z.abs() > lim || !spend.take() {
            return;
        }
        rep.lookups += 1;
        for e in self.grid.cell([cell_of(at.x, c), cell_of(at.y, c), cell_of(at.z, c)]) {
            if !spend.take() {
                return;
            }
            rep.checks += 1;
            let b = &self.nodes[e.node as usize];
            if !matches!(b.phase, Phase::Burning | Phase::Burnt) && box_gap(at, Vec3::ZERO, b.center, b.half) <= 0.0 {
                gain[e.node as usize] += EMBER_HEAT;
            }
        }
    }
}

//! The simulation: nodes, their state, the acts that change it, and the step.

use crate::config::{Config, Material, MAX_FUEL_S};
use crate::error::ElementaError;
use crate::grid::{box_gap, cell_box, cell_of, cells_in, log2_ceil, node_cells, sort_ops, Changes, Grid, MAX_COORD, MAX_NODE_CELLS};
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
    /// For a node ready to catch but held by the burning limit: the tick it became ready
    /// ([`NOT_READY`] otherwise). The longest waiting catch first.
    pub ready_at: u64,
}

/// Where the next spread step starts giving heat: at the first burning node whose id is at
/// least `id`; and, when the work budget cut that node's neighbour query part-way (`cell` and
/// `next` not both 0), within its query at cell `cell` (counted in x, y, z order over the
/// query's cells), from the first neighbour in that cell whose id is at least `next`. Ids, not
/// places in a list, so a node added or removed in between moves nothing; a move that could
/// heat a neighbour twice ends the cut query instead ([`Elementa::move_node`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub id: u64,
    pub cell: u32,
    pub next: u64,
}

impl Cursor {
    /// A query is cut part-way and resumes inside it.
    pub fn is_mid(&self) -> bool {
        self.cell != 0 || self.next != 0
    }

    /// The pass moves on past node `id`: the rest of its cut query is not given.
    fn past(id: u64) -> Cursor {
        Cursor { id: id.wrapping_add(1), cell: 0, next: 0 }
    }
}

/// The cells a burning node's neighbour query covers: its box grown by the reach.
pub fn query_box(n: &Node, config: &Config) -> ([i32; 3], [i32; 3]) {
    let r = Vec3::new(config.reach_m, config.reach_m, config.reach_m);
    cell_box(n.center - n.half - r, n.center + n.half + r, config.cell_m)
}

/// Where a box would be met in a query over `lo..=hi`: the query's cell it is counted in (in x,
/// y, z order) and its id; `None` if it lies in none of the query's cells.
fn query_key(lo: [i32; 3], hi: [i32; 3], center: Vec3, half: Vec3, id: u64, cell: f32) -> Option<(u64, u64)> {
    let (blo, bhi) = cell_box(center - half, center + half, cell);
    if (0..3).any(|a| bhi[a] < lo[a] || blo[a] > hi[a]) {
        return None;
    }
    let f = [0usize, 1, 2].map(|a| i64::from(blo[a].max(lo[a])) - i64::from(lo[a]));
    let (ny, nz) = (i64::from(hi[1]) - i64::from(lo[1]) + 1, i64::from(hi[2]) - i64::from(lo[2]) + 1);
    Some((((f[0] * ny + f[1]) * nz + f[2]) as u64, id))
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
    /// The grid was out of date (nodes added, removed or moved into other cells since the last
    /// spread step), so every node was looked at once instead: an act never updates the grid.
    pub scanned: bool,
}

impl ActReport {
    /// The work and one update a node reached: at most three times the node count, plus one.
    pub fn ops(&self) -> u64 {
        u64::from(self.work) + u64::from(self.nodes)
    }
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
    /// Per kind: how many of `list` (kept with it, so a push costs one step, not a count).
    pub(crate) named: [u32; 5],
}

impl Events {
    pub(crate) fn push(&mut self, e: Event) {
        let k = e.kind as usize;
        if (self.named[k] as usize) < MAX_EVENTS_PER_KIND {
            self.named[k] += 1;
            self.list.push(e);
        } else {
            self.unnamed[k] = self.unnamed[k].saturating_add(1);
        }
    }

    /// Named and counted, per kind.
    pub fn count(&self, kind: EventKind) -> u64 {
        u64::from(self.named[kind as usize]) + u64::from(self.unnamed[kind as usize])
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && self.unnamed.iter().all(|&u| u == 0)
    }

    /// Empties it, keeping its memory.
    pub fn clear(&mut self) {
        self.list.clear();
        self.unnamed = [0; 5];
        self.named = [0; 5];
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
    /// Burning nodes that gave no heat this step because the work budget was spent (their turn
    /// comes first at the next step).
    pub deferred: u32,
    /// Burning nodes whose query the budget cut short (it resumes at the next step).
    pub truncated: u32,
    /// Nodes ready to catch that waited for the burning limit.
    pub waiting: u32,
    /// Nodes ready to catch, put in order (longest waiting first) to choose which catch.
    pub ordered: u32,
    /// Embers thrown and landed whole.
    pub embers: u32,
    /// Embers thrown that the rest of the work budget could not land whole (not given).
    pub embers_dropped: u32,
    pub lookups: u32,
    pub checks: u32,
    /// `lookups + checks`: never above the work limit.
    pub work: u32,
    /// One per node (heat, phase, relaxation).
    pub node_updates: u32,
    /// The sorts of the step, counted as sorts ([`crate::grid::sort_ops`]): the ready nodes, and
    /// the step's events.
    pub sort_ops: u64,
    pub grid_updated: bool,
    pub grid_entries: u32,
    /// The grid brought up to date ([`crate::grid::update_ops`]); 0 when nothing was added,
    /// removed or moved into other cells.
    pub grid_ops: u64,
    pub events: u32,
    /// Operations of the calls made since the last tick (adds, removes, moves and acts), so that
    /// `ops() + calls_ops` is everything the tick cost.
    pub calls_ops: u64,
}

impl StepReport {
    /// Node updates, the sorts, the work and the grid's update: never above
    /// [`crate::config::WorstCase::ops_per_step`] plus [`crate::config::WorstCase::grid_ops`]
    /// (and the grid's part is what [`Elementa::pending_grid_ops`] said before the step).
    pub fn ops(&self) -> u64 {
        u64::from(self.node_updates) + self.sort_ops + u64::from(self.work) + self.grid_ops
    }
}

/// Elementa's whole state for one world.
#[derive(Clone, Debug)]
pub struct Elementa {
    pub(crate) config: Config,
    pub(crate) tick_hz: u32,
    pub(crate) every: u32,
    pub(crate) tick: u64,
    /// The run's key for embers, drawn once from the seed: an ember's chance and range are a
    /// hash of this key, the node's id and the tick, so they never depend on which other nodes
    /// the work budget reached first.
    pub(crate) ember_key: u64,
    /// Sorted by id.
    pub(crate) nodes: Vec<Node>,
    pub(crate) wind: Vec3,
    pub(crate) ambient: f32,
    /// Where the next spread step starts giving heat.
    pub(crate) cursor: Cursor,
    /// Nodes refused or left out for the node limit, all told.
    pub(crate) refused: u64,
    pub(crate) acts: u32,
    pub(crate) events: Events,
    grid: Grid,
    changes: Changes,
    /// Operations of calls since the last tick.
    calls_ops: u64,
    // Buffers kept between steps and acts, so that neither allocates once warm.
    gain: Vec<f32>,
    queue: Vec<(u64, f32, u64, u32)>,
    step_events: Vec<Event>,
    hit: Vec<u32>,
    ranges: Vec<([i32; 3], usize, usize)>,
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
            ember_key: Rng::named(seed, "elementa").next_u64(),
            nodes: Vec::new(),
            wind: Vec3::ZERO,
            ambient: 0.0,
            cursor: Cursor::default(),
            refused: 0,
            acts: 0,
            events: Events { list: Vec::with_capacity(MAX_EVENTS_PER_KIND * EVENT_KINDS.len()), ..Events::default() },
            grid: Grid::default(),
            changes: Changes::default(),
            calls_ops: 0,
            gain: Vec::new(),
            queue: Vec::new(),
            step_events: Vec::new(),
            hit: Vec::new(),
            ranges: Vec::new(),
            report: StepReport::default(),
        })
    }

    /// Room in the step's and the acts' buffers for every node, so that a step or an act never
    /// allocates for the number of nodes (only adding does).
    fn reserve(&mut self) {
        let n = self.nodes.len();
        let grow = |v: usize, want: usize| want.saturating_sub(v);
        self.gain.reserve(grow(self.gain.len(), n));
        self.queue.reserve(grow(self.queue.len(), n));
        self.step_events.reserve(grow(self.step_events.len(), 2 * n));
        self.hit.reserve(grow(self.hit.len(), n));
        self.ranges.reserve(grow(self.ranges.len(), n));
    }

    /// The capacities of every buffer a step or an act uses (a test holds that a warm state's
    /// do not grow).
    pub fn capacities(&self) -> Vec<usize> {
        let mut c = vec![self.gain.capacity(), self.queue.capacity(), self.step_events.capacity(), self.hit.capacity(), self.ranges.capacity()];
        c.extend(self.grid.capacities());
        c.push(self.changes.ids.capacity());
        c.push(self.events.list.capacity());
        c
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

    /// Where the next spread step starts giving heat.
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn last_report(&self) -> &StepReport {
        &self.report
    }

    /// Events since they were last taken (the engine takes them each tick, for the next).
    pub fn take_events(&mut self) -> Events {
        std::mem::take(&mut self.events)
    }

    /// Events since they were last taken, copied into `into` (emptied first; at most 320 named),
    /// this state keeping its own memory: taking events every tick this way allocates nothing
    /// once both are warm.
    pub fn swap_events(&mut self, into: &mut Events) {
        into.clear();
        into.list.extend_from_slice(&self.events.list);
        into.unnamed = self.events.unnamed;
        into.named = self.events.named;
        self.events.clear();
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
        Ok(Node { id: d.id, material: d.material, center: d.center, half: d.half, heat, fuel, wet, phase, since: self.tick, ready_at: NOT_READY })
    }

    /// The most a single add, remove or move costs, in ops, at `n` nodes: a binary search, the
    /// node list shifted by one place, and (a move) the cut query's two keys.
    pub fn call_ops(n: u64) -> u64 {
        n + 2 * log2_ceil(n + 1) + 2
    }

    /// Tracks one node. Refused, with the number, when the node limit is reached. Costs at most
    /// [`Elementa::call_ops`]; the grid takes it in at the next spread step.
    pub fn add(&mut self, d: NodeDesc) -> Result<(), ElementaError> {
        let node = self.make(&d)?;
        let n = self.nodes.len() as u64;
        self.calls_ops += log2_ceil(n + 1) + 1;
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
        self.calls_ops += n - at as u64;
        self.nodes.insert(at, node);
        self.changes.push(d.id, self.nodes.len());
        self.reserve();
        Ok(())
    }

    /// What [`Elementa::add_many`] of `m` nodes costs at most, in ops, at `n` nodes tracked:
    /// each made, sorted, looked up, and merged into the list.
    pub fn add_many_ops(n: u64, m: u64) -> u64 {
        m + sort_ops(m) + m * (log2_ceil(n + 1) + 1) + n + m
    }

    /// Tracks many nodes at once (one sort of the batch and one merge, not one insertion each).
    /// Every node is checked before any is added; past the node limit the highest ids are left
    /// out and counted. Costs at most [`Elementa::add_many_ops`].
    pub fn add_many(&mut self, descs: &[NodeDesc]) -> Result<AddReport, ElementaError> {
        let (n, m) = (self.nodes.len() as u64, descs.len() as u64);
        self.calls_ops += Elementa::add_many_ops(n, m);
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
        let cap = self.nodes.len() + made.len();
        for d in &made {
            self.changes.push(d.id, cap);
        }
        // Both lists are sorted by id, and no id is in both: one merge.
        let old = std::mem::take(&mut self.nodes);
        let mut merged = Vec::with_capacity(cap);
        let (mut a, mut b) = (old.into_iter().peekable(), made.into_iter().peekable());
        loop {
            let take_a = match (a.peek(), b.peek()) {
                (Some(x), Some(y)) => x.id < y.id,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            merged.extend(if take_a { a.next() } else { b.next() });
        }
        self.nodes = merged;
        self.reserve();
        Ok(AddReport { added, left_out: left_out as u32, limit: max as u32 })
    }

    /// Stops tracking a node; `false` if it was not tracked. Costs at most
    /// [`Elementa::call_ops`]; the grid lets it go at the next spread step. If the work budget had
    /// cut this node's own query, the rest of that query is not given.
    pub fn remove(&mut self, id: u64) -> bool {
        let n = self.nodes.len() as u64;
        self.calls_ops += log2_ceil(n + 1) + 1;
        match self.nodes.binary_search_by_key(&id, |n| n.id) {
            Ok(i) => {
                self.calls_ops += n - i as u64;
                if self.nodes.remove(i).phase == Phase::Burning {
                    self.report.burning = self.report.burning.saturating_sub(1);
                }
                if self.cursor.is_mid() && self.cursor.id == id {
                    self.cursor = Cursor::past(id);
                }
                self.changes.push(id, self.nodes.len());
                true
            }
            Err(_) => false,
        }
    }

    /// A node's entity moved or changed size. Costs at most [`Elementa::call_ops`]. A node that
    /// stays in the cells it covered changes nothing in the grid; one that moves into others is
    /// taken in at the next spread step.
    ///
    /// While the work budget has a query cut part-way, a move that could make that query heat a
    /// neighbour twice ends it instead (the rest of its heat is not given, and the pass moves
    /// on): the cut node itself moving (its query's cells change), or a neighbour the query has
    /// already passed moving to where it has not yet been.
    pub fn move_node(&mut self, id: u64, center: Vec3, half: Vec3) -> Result<(), ElementaError> {
        check_box(&self.config, id, center, half)?;
        let n = self.nodes.len() as u64;
        self.calls_ops += log2_ceil(n + 1) + 1;
        let i = self.index(id)?;
        let (old_c, old_h) = (self.nodes[i].center, self.nodes[i].half);
        if self.cursor.is_mid() {
            let cut = self.cursor;
            if cut.id == id {
                self.cursor = Cursor::past(id);
            } else if let Ok(s) = self.nodes.binary_search_by_key(&cut.id, |n| n.id) {
                self.calls_ops += log2_ceil(n + 1) + 1;
                let (lo, hi) = query_box(&self.nodes[s], &self.config);
                let at = (u64::from(cut.cell), cut.next);
                let c = self.config.cell_m;
                let passed = query_key(lo, hi, old_c, old_h, id, c).is_some_and(|k| k < at);
                let ahead = query_key(lo, hi, center, half, id, c).is_some_and(|k| k >= at);
                if passed && ahead {
                    self.cursor = Cursor::past(cut.id);
                }
            }
        }
        let before = node_cells(&self.nodes[i], self.config.cell_m);
        self.nodes[i].center = center;
        self.nodes[i].half = half;
        if node_cells(&self.nodes[i], self.config.cell_m) != before {
            self.changes.push(id, self.nodes.len());
        }
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

    /// After a load: the grid is made whole at the next spread step, and the buffers sized.
    pub(crate) fn mark_all_changed(&mut self) {
        self.changes.everything();
        self.reserve();
    }

    /// What bringing the grid up to date at the next spread step will cost (0 when it is up to
    /// date): said before it runs ([`crate::grid::update_ops`]).
    pub fn pending_grid_ops(&self) -> u64 {
        self.grid.pending_ops(&self.nodes, &self.changes, self.config.cell_m)
    }

    /// The grid holds every node where it is now (no add, remove or move into other cells since
    /// the last spread step).
    pub fn grid_is_current(&self) -> bool {
        self.changes.is_empty()
    }

    /// Forgets the tick's acts (a copy made for a prediction starts with none).
    pub(crate) fn clear_acts(&mut self) {
        self.acts = 0;
    }

    /// Applies an act at once. At most [`MAX_ACTS_PER_TICK`] between two ticks; its sphere at
    /// most [`MAX_ACT_RADIUS`]. Its work is at most twice the node count plus one: the sphere's
    /// cells are looked up first (only when there are no more of them than nodes), and when
    /// they hold more entries than there are nodes, or the grid is out of date, every node is
    /// looked at once instead. An act never updates the grid (only a spread step does), so acts
    /// between adds and moves cost no grid work at all ([`ActReport::ops`]).
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
        let mut hit = std::mem::take(&mut self.hit);
        hit.clear();
        let reached = self.reach(a, &mut hit, &mut report);
        if let Err(e) = reached {
            self.hit = hit;
            return Err(e);
        }
        for &i in &hit {
            let n = &mut self.nodes[i as usize];
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
        self.hit = hit;
        self.calls_ops += report.ops();
        Ok(report)
    }

    /// The nodes an act reaches, into `hit` (each once; the order does not matter, as an act
    /// changes each node by itself).
    fn reach(&mut self, a: Act, hit: &mut Vec<u32>, report: &mut ActReport) -> Result<(), ElementaError> {
        match a.target {
            Target::Node(id) => {
                hit.push(self.index(id)? as u32);
                report.work = 1;
            }
            Target::Sphere { at, radius } => {
                if !finite3(at) || at.x.abs() > MAX_COORD || at.y.abs() > MAX_COORD || at.z.abs() > MAX_COORD {
                    return Err(ElementaError::range(&format!("{}.at", a.kind.name()), f64::from(at.length()), 0.0, f64::from(MAX_COORD)));
                }
                if !radius.is_finite() || !(0.0..=MAX_ACT_RADIUS).contains(&radius) {
                    return Err(ElementaError::range(&format!("{}.radius", a.kind.name()), f64::from(radius), 0.0, f64::from(MAX_ACT_RADIUS)));
                }
                let r = Vec3::new(radius, radius, radius);
                let (clo, chi) = cell_box(at - r, at + r, self.config.cell_m);
                let within = |n: &Node| box_gap(at, Vec3::ZERO, n.center, n.half) <= radius;
                let n = self.nodes.len() as u64;
                let cells = cells_in(clo, chi);
                let current = self.changes.is_empty();
                self.ranges.clear();
                let mut entries = 0u64;
                if current && cells <= n {
                    for x in clo[0]..=chi[0] {
                        for y in clo[1]..=chi[1] {
                            for z in clo[2]..=chi[2] {
                                report.work += 1;
                                let (a, b) = self.grid.range([x, y, z]);
                                entries += (b - a) as u64;
                                self.ranges.push(([x, y, z], a, b));
                            }
                        }
                    }
                }
                if !current || cells > n || cells + entries > n {
                    // The grid out of date, or more cells or more entries in them than there
                    // are nodes: every node once.
                    report.scanned = !current;
                    for (i, node) in self.nodes.iter().enumerate() {
                        report.work += 1;
                        if within(node) {
                            hit.push(i as u32);
                        }
                    }
                } else {
                    let all = self.grid.entries();
                    for &(c, a, b) in &self.ranges {
                        for e in &all[a..b] {
                            report.work += 1;
                            let node = &self.nodes[e.node as usize];
                            if first_shared_cell(node, clo, self.config.cell_m) == c && within(node) {
                                hit.push(e.node);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }


    /// Advances one tick. Every `ticks_per_step` ticks a spread step runs: the grid is brought
    /// up to date, burning things heat their neighbours, phases change, everything relaxes, and
    /// events are queued.
    pub fn step(&mut self) -> &StepReport {
        self.tick = self.tick.saturating_add(1).min(MAX_TICK);
        self.acts = 0;
        let calls_ops = std::mem::take(&mut self.calls_ops);
        if !self.tick.is_multiple_of(u64::from(self.every)) {
            // Nothing moves between spread steps: the burning count is the last step's.
            self.report = StepReport { tick: self.tick, nodes: self.nodes.len() as u32, burning: self.report.burning, calls_ops, ..StepReport::default() };
            return &self.report;
        }
        self.spread();
        self.report.calls_ops = calls_ops;
        &self.report
    }

    fn spread(&mut self) {
        let dt = 1.0 / self.config.spread_hz as f32;
        let mut rep = StepReport { tick: self.tick, spread: true, nodes: self.nodes.len() as u32, ..StepReport::default() };
        rep.grid_ops = self.grid.update(&self.nodes, &mut self.changes, self.config.cell_m);
        rep.grid_updated = rep.grid_ops > 0;
        rep.grid_entries = self.grid.len() as u32;
        let n = self.nodes.len();
        self.gain.clear();
        self.gain.resize(n, 0.0);

        // Heat given: burning nodes in id order from the cursor, round to the start, within the
        // work budget, each at most once a step. A burning node's query gives each neighbour it
        // reaches one step of heat; a query the budget cuts resumes at the next step where it
        // stopped, so every neighbour of one pass is given the same, once. What a node the
        // budget did not reach this step would have given is not given: a limit can only slow a
        // fire. Gains are summed first and applied after, so no node's place in the order
        // changes another's result.
        let mut spend = Spend { used: 0, budget: u64::from(self.config.limits.work) };
        let resume = self.cursor;
        let start = self.nodes.partition_point(|x| x.id < resume.id);
        let mut next_cursor = None;
        for k in 0..n {
            let i = (start + k) % n;
            if self.nodes[i].phase != Phase::Burning {
                continue;
            }
            let id = self.nodes[i].id;
            if spend.used >= spend.budget {
                next_cursor.get_or_insert(Cursor { id, cell: 0, next: 0 });
                rep.deferred += 1;
                continue;
            }
            rep.processed += 1;
            let from = if id == resume.id { (resume.cell, resume.next) } else { (0, 0) };
            let q = Query { nodes: &self.nodes, grid: &self.grid, config: &self.config, wind: self.wind };
            match q.heat(i, dt, from, &mut self.gain, &mut spend, &mut rep) {
                Some((cell, next)) => {
                    rep.truncated += 1;
                    next_cursor.get_or_insert(Cursor { id, cell, next });
                }
                None => {
                    // The query is done: its ember, if it throws one, lands whole or not at all.
                    if let Some(at) = self.ember_at(i, dt) {
                        if q.ember(at, &mut self.gain, &mut spend, &mut rep) {
                            rep.embers += 1;
                        } else {
                            rep.embers_dropped += 1;
                        }
                    }
                }
            }
        }
        self.cursor = next_cursor.unwrap_or_default();
        rep.work = spend.used as u32;

        // Phases: first what ends (out, burnt, frozen, thawed), in id order; then what catches.
        let tick = self.tick;
        let mut events = std::mem::take(&mut self.step_events);
        events.clear();
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
                        events.push(ev(EventKind::Extinguished, node, Some(by)));
                    } else if node.fuel <= 0.0 {
                        node.phase = Phase::Burnt;
                        node.since = tick;
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
        // A cut query whose node no longer burns gives up the rest of it.
        if self.cursor.is_mid() {
            let burns = self.node(self.cursor.id).is_some_and(|x| x.phase == Phase::Burning);
            if !burns {
                self.cursor = Cursor::past(self.cursor.id);
            }
        }
        // What catches: every ready node, longest waiting first, then hottest, then in an order
        // drawn afresh each step from the ids (a hash of id and tick), never by id itself: under
        // the burning limit the fire then grows where it was ready first, whichever way the
        // entities' ids run.
        let burning = self.burning();
        let limit = self.config.limits.burning;
        let mut queue = std::mem::take(&mut self.queue);
        queue.clear();
        for (j, node) in self.nodes.iter_mut().enumerate() {
            let m = &self.config.materials[usize::from(node.material)];
            if ready(node, m) {
                if node.ready_at == NOT_READY {
                    node.ready_at = tick;
                }
                queue.push((node.ready_at, node.heat, avalanche(node.id ^ tick.wrapping_mul(0x9e37_79b9_7f4a_7c15)), j as u32));
            } else {
                node.ready_at = NOT_READY;
            }
        }
        rep.ordered = queue.len() as u32;
        rep.sort_ops = sort_ops(queue.len() as u64);
        queue.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
        let room = limit.saturating_sub(burning) as usize;
        for &(_, _, _, j) in queue.iter().take(room) {
            let node = &mut self.nodes[j as usize];
            node.phase = Phase::Burning;
            node.since = tick;
            node.ready_at = NOT_READY;
            events.push(ev(EventKind::Ignited, node, None));
        }
        rep.waiting = queue.len().saturating_sub(room) as u32;
        let burning = burning + queue.len().min(room) as u32;
        self.queue = queue;
        for node in self.nodes.iter_mut() {
            let m = &self.config.materials[usize::from(node.material)];
            relax(node, m, self.ambient, dt);
        }
        rep.sort_ops += sort_ops(events.len() as u64);
        events.sort_unstable_by_key(|e| (e.id, e.kind));
        rep.events = events.len() as u32;
        for e in &events {
            self.events.push(*e);
        }
        self.step_events = events;
        rep.burning = burning;
        self.report = rep;
    }

    /// Whether burning node `i`, its query done this step, throws an ember in this wind, and
    /// where it lands: a draw keyed by the run's ember key, the node's id and the tick (never by
    /// which nodes the budget reached before it), with chance `embers × dt`.
    fn ember_at(&self, i: usize, dt: f32) -> Option<Vec3> {
        let n = &self.nodes[i];
        let m = &self.config.materials[usize::from(n.material)];
        let speed = self.wind.length();
        if m.embers <= 0.0 || speed < EMBER_WIND {
            return None;
        }
        let draw = avalanche(self.ember_key ^ avalanche(n.id.wrapping_add(avalanche(self.tick))));
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

    fn left(&self) -> u64 {
        self.budget.saturating_sub(self.used)
    }
}

impl Query<'_> {
    /// Heats the neighbours of burning node `i` with one step's heat each, starting at `from`
    /// (a cell, counted in x, y, z order over the query's cells, and the least id to begin at in
    /// it). `Some(where to resume)` when the budget cut it short; `None` when every cell was
    /// looked at.
    fn heat(&self, i: usize, dt: f32, from: (u32, u64), gain: &mut [f32], spend: &mut Spend, rep: &mut StepReport) -> Option<(u32, u64)> {
        let a = &self.nodes[i];
        let m = &self.config.materials[usize::from(a.material)];
        if m.heat_out <= 0.0 {
            return None;
        }
        let reach = self.config.reach_m;
        let (lo, hi) = query_box(a, self.config);
        let mut k = 0u32;
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    let this = k;
                    k += 1;
                    if this < from.0 {
                        continue;
                    }
                    let at = if this == from.0 { from.1 } else { 0 };
                    if !spend.take() {
                        return Some((this, at));
                    }
                    rep.lookups += 1;
                    let entries = self.grid.cell([x, y, z]);
                    let mut first = 0;
                    if at > 0 {
                        // Resuming inside this cell: a binary search by id, counted.
                        for _ in 0..log2_ceil(entries.len() as u64 + 1) {
                            if !spend.take() {
                                return Some((this, at));
                            }
                            rep.lookups += 1;
                        }
                        first = entries.partition_point(|e| self.nodes[e.node as usize].id < at);
                    }
                    for e in &entries[first..] {
                        let j = e.node as usize;
                        if !spend.take() {
                            return Some((this, self.nodes[j].id));
                        }
                        rep.checks += 1;
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

    /// An ember lands at `at`: the things whose boxes hold that point are given its heat, if the
    /// budget left holds the lookup and every entry of its cell; otherwise none is (`false`).
    fn ember(&self, at: Vec3, gain: &mut [f32], spend: &mut Spend, rep: &mut StepReport) -> bool {
        let c = self.config.cell_m;
        let lim = MAX_COORD + MAX_HALF * 2.0;
        if at.x.abs() > lim || at.y.abs() > lim || at.z.abs() > lim || !spend.take() {
            return false;
        }
        rep.lookups += 1;
        let entries = self.grid.cell([cell_of(at.x, c), cell_of(at.y, c), cell_of(at.z, c)]);
        if (entries.len() as u64) > spend.left() {
            return false;
        }
        for e in entries {
            spend.take();
            rep.checks += 1;
            let b = &self.nodes[e.node as usize];
            if !matches!(b.phase, Phase::Burning | Phase::Burnt) && box_gap(at, Vec3::ZERO, b.center, b.half) <= 0.0 {
                gain[e.node as usize] += EMBER_HEAT;
            }
        }
        true
    }
}

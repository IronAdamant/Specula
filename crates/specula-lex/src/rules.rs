//! The compiled form of a rule set: indices instead of names, integers in stored units,
//! expressions type-checked and folded. Built by [`crate::compile`], run by [`crate::Sim`].

use specula_core::Json;

use crate::expr::Ex;
use crate::units::Unit;

/// A position in millimetres (right-handed, +Y up).
pub type P3 = [i64; 3];

/// Built-in events, in this order, before any custom events.
pub const BUILTIN_EVENTS: [(&str, Unit, &str); 11] = [
    ("spawned", Unit::Int, "an entity appeared (subject)"),
    ("died", Unit::Int, "an entity died (subject; other: the killer, when known)"),
    ("removed", Unit::Int, "an entity was despawned without dying (subject)"),
    ("arrived", Unit::Int, "a follow_path entity reached the end of its path (subject)"),
    ("damaged", Unit::Points, "an entity lost health (subject, other: the source; value: the damage)"),
    ("hit", Unit::Int, "a payload reached an entity (subject: the victim, other: the attacker)"),
    ("effect_applied", Unit::Int, "an effect was applied (subject; value: the magnitude)"),
    ("effect_expired", Unit::Int, "an effect ran out (subject)"),
    ("wave_started", Unit::Int, "a wave began (value: its number, from 1)"),
    ("wave_cleared", Unit::Int, "every enemy of a wave is gone (value: its number)"),
    ("state_entered", Unit::Int, "a machine entered a state (subject)"),
];

pub mod ev {
    pub const SPAWNED: u16 = 0;
    pub const DIED: u16 = 1;
    pub const REMOVED: u16 = 2;
    pub const ARRIVED: u16 = 3;
    pub const DAMAGED: u16 = 4;
    pub const HIT: u16 = 5;
    pub const EFFECT_APPLIED: u16 = 6;
    pub const EFFECT_EXPIRED: u16 = 7;
    pub const WAVE_STARTED: u16 = 8;
    pub const WAVE_CLEARED: u16 = 9;
    pub const STATE_ENTERED: u16 = 10;
}

#[derive(Clone, Debug)]
pub struct Rules {
    pub name: String,
    pub tick_hz: u32,
    /// The document's hash with the param overrides folded in.
    pub hash: u64,
    pub attrs: Vec<AttrDef>,
    pub resources: Vec<ResourceDef>,
    pub params: Vec<ParamDef>,
    pub paths: Vec<PathDef>,
    pub sites: Vec<SiteDef>,
    /// The `sites` items, by id: [`SiteDef::group`] indexes this.
    pub site_groups: Vec<String>,
    pub tags: Vec<String>,
    pub streams: Vec<String>,
    pub archetypes: Vec<ArchDef>,
    pub effects: Vec<EffectDef>,
    pub machines: Vec<MachineDef>,
    pub waves: Option<WavesDef>,
    pub timers: Vec<TimerDef>,
    pub rules: Vec<RuleDef>,
    pub commands: Vec<CommandDef>,
    /// By priority (highest first), then id.
    pub outcomes: Vec<OutcomeDef>,
    pub natives: Vec<NativeDef>,
    /// Built-ins ([`BUILTIN_EVENTS`]) then custom events.
    pub events: Vec<EventDef>,
    /// `data` items, by id: tables for native hooks (a map, a curve).
    pub data: Vec<DataDef>,
    /// Some archetype declares an `order`: behaviour phases run entities by (order, id).
    pub ordered: bool,
    /// Some wave group gives its spawns `with` values (they are then part of the state hash).
    pub with_groups: bool,
}

impl Rules {
    pub fn archetype(&self, id: &str) -> Option<u16> {
        self.archetypes.iter().position(|a| a.id == id).map(|i| i as u16)
    }

    pub fn resource(&self, id: &str) -> Option<u16> {
        self.resources.iter().position(|r| r.id == id).map(|i| i as u16)
    }

    pub fn tag(&self, name: &str) -> Option<u16> {
        self.tags.iter().position(|t| t == name).map(|i| i as u16)
    }

    pub fn site(&self, name: &str) -> Option<u16> {
        self.sites.iter().position(|s| s.name == name).map(|i| i as u16)
    }

    pub fn command(&self, id: &str) -> Option<u16> {
        self.commands.iter().position(|c| c.id == id).map(|i| i as u16)
    }

    pub fn event(&self, name: &str) -> Option<u16> {
        self.events.iter().position(|e| e.name == name).map(|i| i as u16)
    }

    /// An attribute's id (natives resolve names once, when they bind).
    pub fn attr(&self, name: &str) -> Option<u16> {
        self.attrs.iter().position(|a| a.name == name).map(|i| i as u16)
    }

    /// A random stream's id (natives declare theirs in `streams`).
    pub fn stream(&self, name: &str) -> Option<u16> {
        self.streams.iter().position(|s| s == name).map(|i| i as u16)
    }

    /// A `data` item by id.
    pub fn data(&self, id: &str) -> Option<&DataDef> {
        self.data.iter().find(|d| d.id == id)
    }
}

/// A `data` item: JSON a native hook reads (the rules' hash covers it).
#[derive(Clone, Debug)]
pub struct DataDef {
    pub id: String,
    pub value: Json,
    /// A stable hash of the canonical value, so natives can cache what they derive from it.
    pub hash: u64,
}

#[derive(Clone, Debug)]
pub struct AttrDef {
    pub name: String,
    pub unit: Unit,
}

#[derive(Clone, Debug)]
pub struct ResourceDef {
    pub id: String,
    pub unit: Unit,
    pub start: i64,
    pub min: i64,
    pub max: i64,
}

#[derive(Clone, Debug)]
pub struct ParamDef {
    pub id: String,
    pub unit: Unit,
    pub value: i64,
    pub min: i64,
    pub max: i64,
    pub doc: String,
}

#[derive(Clone, Debug)]
pub struct PathDef {
    pub id: String,
    pub points: Vec<P3>,
    /// Segment lengths (mm, rounded down); `seg[i]` runs from `points[i]` to `points[i + 1]`.
    pub seg: Vec<i64>,
    /// The length after segment `i`: `seg[i + 1..]` summed.
    pub after: Vec<i64>,
    pub looped: bool,
}

#[derive(Clone, Debug)]
pub struct SiteDef {
    pub name: String,
    pub pos: P3,
    /// The `sites` item it comes from (index into [`Rules::site_groups`]).
    pub group: u16,
}

#[derive(Clone, Debug)]
pub struct ArchDef {
    pub id: String,
    /// Sorted tag ids.
    pub tags: Vec<u16>,
    /// (attribute id, base value), sorted by attribute id; an entity's attributes are a parallel `Vec`.
    pub attrs: Vec<(u16, i64)>,
    /// Attribute id → index in `attrs` (`u16::MAX` = none), one per [`Rules::attrs`].
    pub slot: Vec<u16>,
    pub behaviours: Vec<Beh>,
    pub machine: Option<u16>,
    pub next: Option<u16>,
    pub abstract_: bool,
    pub on_spawn: Vec<Action>,
    /// The health behaviour's attribute, when it has one.
    pub health: Option<u16>,
    /// Evaluated once, after `on_spawn`: behaviour phases run entities by (order, id).
    pub order: Option<Ex>,
}

impl ArchDef {
    pub fn slot_of(&self, attr: u16) -> Option<usize> {
        match self.slot.get(usize::from(attr)) {
            Some(&s) if s != u16::MAX => Some(usize::from(s)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Beh {
    pub name: String,
    pub kind: BehKind,
    /// Machine states this behaviour runs in; `None` = always.
    pub active_in: Option<Vec<u16>>,
}

#[derive(Clone, Debug)]
pub enum BehKind {
    Health { attr: u16, on_death: Vec<Action> },
    FollowPath { path: PathSel, speed: Ex, scale: Option<Ex>, on_arrive: Vec<Action> },
    Patrol { path: u16, speed: Ex, scale: Option<Ex>, pingpong: bool },
    Chase { targets: TagFilter, range: Ex, speed: Ex, scale: Option<Ex>, reach: Ex, on_reach: Vec<Action> },
    Attack(Box<AttackDef>),
    Projectile { speed: Ex, homing: bool, splash: Ex },
    Spawner { arch: u16, every: Ex, count: Ex, start: Ex, with: Vec<(u16, Ex)> },
    Lifetime { duration: Ex, on_expire: Vec<Action> },
    /// `on`: named action lists the hook runs with `NativeCtx::trigger` (self: the entity).
    Native { hook: u16, args: Vec<(String, Ex)>, on: Vec<(String, Vec<Action>)> },
}

impl BehKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            BehKind::Health { .. } => "health",
            BehKind::FollowPath { .. } => "follow_path",
            BehKind::Patrol { .. } => "patrol",
            BehKind::Chase { .. } => "chase",
            BehKind::Attack(_) => "attack",
            BehKind::Projectile { .. } => "projectile",
            BehKind::Spawner { .. } => "spawner",
            BehKind::Lifetime { .. } => "lifetime",
            BehKind::Native { .. } => "native",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathSel {
    Fixed(u16),
    /// The path the entity was spawned on.
    Spawn,
}

/// `all` must be present, `none` absent. A `u16::MAX` in `all` never matches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TagFilter {
    pub all: Vec<u16>,
    pub none: Vec<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    First,
    Last,
    Closest,
    Strongest,
    Weakest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Instant,
    Pulse,
    Projectile,
}

#[derive(Clone, Debug)]
pub struct AttackDef {
    pub targets: TagFilter,
    /// An extra eligibility test per candidate (self: the attacker, target: the candidate).
    /// Splash doesn't use it: a landing hits what is near.
    pub when: Option<Ex>,
    pub range: Ex,
    pub cooldown: Ex,
    pub policy: Policy,
    pub delivery: Delivery,
    /// The projectile archetype and attributes set on it when fired.
    pub projectile: Option<(u16, Vec<(u16, Ex)>)>,
    pub muzzle: P3,
    pub blocked_by: Vec<u16>,
    pub payload: Vec<Payload>,
}

#[derive(Clone, Debug)]
pub enum Payload {
    Damage(Ex),
    Effect { effect: u16, magnitude: Option<Ex>, duration: Option<Ex> },
}

#[derive(Clone, Debug)]
pub enum EffDur {
    Instant,
    Ticks(Ex),
    Infinite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModOp {
    Add,
    Mul,
    Set,
}

#[derive(Clone, Debug)]
pub struct Modifier {
    pub attr: u16,
    pub op: ModOp,
    pub value: Ex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stacking {
    Strongest,
    Refresh,
    Stack,
    Independent,
    Ignore,
}

#[derive(Clone, Debug)]
pub struct EffectDef {
    pub id: String,
    pub duration: EffDur,
    pub period: Option<Ex>,
    pub magnitude_unit: Unit,
    pub modifiers: Vec<Modifier>,
    pub stacking: Stacking,
    pub max_stacks: u16,
    pub grant_tags: Vec<u16>,
    pub block_tags: Vec<u16>,
    pub on_apply: Vec<Action>,
    pub on_period: Vec<Action>,
    pub on_expire: Vec<Action>,
}

#[derive(Clone, Debug)]
pub struct MachineDef {
    pub id: String,
    pub initial: u16,
    pub states: Vec<StateDef>,
}

#[derive(Clone, Debug)]
pub struct StateDef {
    pub name: String,
    /// Behaviour names active in this state.
    pub behaviours: Vec<String>,
    pub on_enter: Vec<Action>,
    pub on_exit: Vec<Action>,
    pub transitions: Vec<Transition>,
}

#[derive(Clone, Debug)]
pub struct Transition {
    pub to: u16,
    pub when: Option<Ex>,
    /// Ticks in the state before it may fire.
    pub after: Option<Ex>,
    /// Fires when this event happens to the entity.
    pub on: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct WavesDef {
    /// Ticks before wave 1.
    pub start: u32,
    pub stream: u16,
    pub waves: Vec<WaveDef>,
}

#[derive(Clone, Debug)]
pub struct WaveDef {
    /// Ticks until the next wave starts on its own (`None`: the last wave).
    pub duration: Option<u32>,
    pub on_start: Vec<Action>,
    /// Attribute ratios (permille) applied to spawned entities' base values.
    pub scale: Vec<(u16, i64)>,
    pub groups: Vec<GroupDef>,
}

#[derive(Clone, Debug)]
pub struct GroupDef {
    pub arch: u16,
    pub count: u32,
    pub at: Vec<SpawnAt>,
    pub start: u32,
    pub every: u32,
    /// Attribute values for each spawn, evaluated when the wave is scheduled, right after its
    /// spawn point is drawn, in attribute-name order.
    pub with: Vec<(u16, Ex)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnAt {
    Path(u16),
    Site(u16),
}

#[derive(Clone, Debug)]
pub struct TimerDef {
    pub id: String,
    pub start: u32,
    pub every: Option<u32>,
    pub count: Option<u32>,
    pub when: Option<Ex>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug)]
pub struct RuleDef {
    pub id: String,
    pub on: u16,
    pub tags: TagFilter,
    pub when: Option<Ex>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug)]
pub enum ArgTy {
    /// A site; `Some(group)`: one from that `sites` item.
    Site(Option<u16>),
    Archetype(TagFilter),
    Entity(TagFilter),
    Num(Unit),
}

#[derive(Clone, Debug)]
pub struct ArgDef {
    pub name: String,
    pub ty: ArgTy,
}

#[derive(Clone, Debug)]
pub struct Require {
    pub cond: Ex,
    pub code: String,
    pub message: String,
    /// Numbers the refusal carries (`need`, `have`), evaluated when it refuses.
    pub details: Vec<(String, Ex)>,
}

#[derive(Clone, Debug)]
pub struct CommandDef {
    pub id: String,
    pub args: Vec<ArgDef>,
    pub cost: Vec<(u16, Ex)>,
    pub require: Vec<Require>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug)]
pub struct OutcomeDef {
    pub id: String,
    pub result: String,
    pub when: Ex,
    pub priority: i64,
    /// Reads no entity scans (count, occupied), so it is checked after every entity's step.
    pub cheap: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Spawn,
    Move,
    Act,
    Effects,
    /// Actions only (not a behaviour phase).
    Any,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Spawn => "spawn",
            Phase::Move => "move",
            Phase::Act => "act",
            Phase::Effects => "effects",
            Phase::Any => "any",
        }
    }
}

#[derive(Clone, Debug)]
pub struct NativeDef {
    pub id: String,
    pub hook: String,
    pub reason: String,
    pub phase: Phase,
    /// Random streams the hook draws from (by id).
    pub streams: Vec<u16>,
    /// `data` items the hook reads (indices into [`Rules::data`]).
    pub data: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct EventDef {
    pub name: String,
    pub unit: Unit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeOp {
    By,
    To,
}

/// Where a spawn appears.
#[derive(Clone, Debug)]
pub enum At {
    Path(u16),
    /// A site value.
    Site(Ex),
    /// An entity's position (continuing its path, if it follows one).
    Entity(Ex),
}

#[derive(Clone, Debug)]
pub enum Action {
    AddRes { res: u16, amount: Ex },
    SetRes { res: u16, value: Ex },
    Change { who: Ex, attr: u16, op: ChangeOp, value: Ex },
    Spawn { arch: Ex, at: At, count: Ex, with: Vec<(u16, Ex)> },
    Despawn(Ex),
    Kill(Ex),
    Damage { to: Ex, amount: Ex },
    Apply { effect: u16, to: Ex, magnitude: Option<Ex>, duration: Option<Ex> },
    Emit { event: u16, subject: Option<Ex>, value: Option<Ex> },
    Become { who: Ex, arch: Ex, keep: Vec<u16> },
    NextWave,
    /// Machine id and state.
    Goto { who: Ex, machine: u16, state: u16 },
    Outcome(String),
    If { cond: Ex, then: Vec<Action>, otherwise: Vec<Action> },
    Native { hook: u16, args: Vec<(String, Ex)> },
}

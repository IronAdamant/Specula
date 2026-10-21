//! Crowds: agents that follow paths on a navmesh and avoid each other, one fixed step at a time
//! (docs/design/NAVIGATION.md, "Crowds").
//!
//! Each step: agents find their polygon, keep or request a corridor (a path's polygons), repaths
//! are served within a per-step budget (round robin by id), each agent steers toward its next
//! corner, picks a velocity from a fixed table of samples scored against its neighbours' (time to
//! collision, reciprocal), and moves along the surface; overlapping agents are pushed apart.
//! Agents moved by something else (a physics character) only get a velocity; their position is
//! fed back before the next step.
//!
//! Determinism: agents are visited by id; neighbours sort by distance then id; the sample table
//! is built with `dmath`; ties go to the earlier sample.

use crate::mesh::{NavMesh, PolyRef};
use crate::query::horizontal_distance;
use crate::{Filter, LinkKind, NavError};
use specula_core::dmath;
use specula_core::hash::StableHasher;
use specula_core::math::Vec3;
use specula_core::Json;
use std::collections::BTreeMap;

/// Most agents one crowd holds.
pub const MAX_AGENTS: usize = 1024;
/// Velocity samples: directions × speeds, plus the preferred velocity and standing still.
const DIRECTIONS: usize = 16;
const SPEEDS: [f32; 2] = [1.0, 0.5];
/// Neighbour search cell size, metres.
const GRID: f32 = 4.0;

/// Agents one agent looks at per row of grid cells, at most (bounds a clump's cost: without it a
/// thousand agents at one point cost a million distance checks and sorts per step).
const MAX_SCAN: usize = 128;

/// An agent as one step's neighbour search sees it.
#[derive(Clone, Copy, Debug)]
struct Snap {
    id: u64,
    pos: Vec3,
    vel: Vec3,
    radius: f32,
    reciprocal: bool,
    on_mesh: bool,
}

/// Buffers a step reuses (reset before use; no state between steps).
#[derive(Clone, Debug, Default)]
struct Scratch {
    snap: Vec<Snap>,
    /// ((row, column), agent): sorted, so a row's neighbouring cells are contiguous.
    grid: Vec<((i32, i32), u32)>,
    near: Vec<(f32, u32)>,
    others: Vec<Other>,
    chosen: Vec<Vec3>,
    push: Vec<Vec3>,
    order: Vec<u64>,
}

/// A neighbour as avoidance sees it.
#[derive(Clone, Copy, Debug)]
struct Other {
    pos: Vec3,
    vel: Vec3,
    radius: f32,
    /// It avoids too (each takes half the turn).
    reciprocal: bool,
}

/// Crowd-wide settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrowdSettings {
    /// Path searches served per step.
    pub repaths_per_step: usize,
    /// Polygons path searches may expand per step, all together (at least one search runs).
    pub nodes_per_step: usize,
    /// Polygons a corridor keeps.
    pub max_corridor: usize,
    /// Polygons one path search expands.
    pub max_nodes: usize,
    /// A corridor older than this is searched again (seconds).
    pub repath_s: f32,
    /// A target that moved this far from where the corridor leads gets a new path (metres).
    pub repath_distance: f32,
    /// Neighbours each agent avoids.
    pub neighbours: usize,
    /// How far ahead avoidance looks (seconds).
    pub horizon_s: f32,
    /// Search box half sizes for finding an agent's polygon.
    pub extent: Vec3,
}

impl Default for CrowdSettings {
    fn default() -> CrowdSettings {
        CrowdSettings {
            repaths_per_step: 32,
            nodes_per_step: 2048,
            max_corridor: 256,
            max_nodes: 4096,
            repath_s: 8.0,
            repath_distance: 0.5,
            neighbours: 6,
            horizon_s: 2.0,
            extent: Vec3::new(1.0, 2.0, 1.0),
        }
    }
}

/// What an agent is and how it moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AgentParams {
    pub radius: f32,
    /// Top speed, m/s.
    pub speed: f32,
    /// Largest change of velocity, m/s².
    pub accel: f32,
    /// It has arrived within this distance of its target (metres).
    pub arrive: f32,
    /// It avoids other agents (others avoid it either way).
    pub avoid: bool,
    /// Moved by something else (a physics character): the crowd only sets its velocity.
    pub external: bool,
}

impl Default for AgentParams {
    fn default() -> AgentParams {
        AgentParams { radius: 0.3, speed: 2.0, accel: 8.0, arrive: 0.25, avoid: true, external: false }
    }
}

/// Where an agent is in its errand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgentStatus {
    /// No target.
    Idle,
    /// Waiting for its path search (the per-step budget).
    Waiting,
    Moving,
    Arrived,
    /// Moving to the reachable point nearest a target it can't reach.
    Partial,
    /// Not on the navmesh (nothing near it within the search box).
    OffMesh,
}

impl AgentStatus {
    pub fn name(self) -> &'static str {
        match self {
            AgentStatus::Idle => "idle",
            AgentStatus::Waiting => "waiting",
            AgentStatus::Moving => "moving",
            AgentStatus::Arrived => "arrived",
            AgentStatus::Partial => "partial",
            AgentStatus::OffMesh => "off_mesh",
        }
    }
}

/// One agent's state.
#[derive(Clone, Debug, PartialEq)]
pub struct CrowdAgent {
    pub params: AgentParams,
    pub pos: Vec3,
    pub vel: Vec3,
    /// The velocity it steered toward this step (before avoidance).
    pub desired: Vec3,
    pub poly: Option<PolyRef>,
    /// The navmesh's build counter when `poly` was found.
    poly_at: u64,
    pub target: Option<Vec3>,
    pub status: AgentStatus,
    pub corridor: Vec<PolyRef>,
    /// Where the corridor ends (on the navmesh).
    pub path_end: Vec3,
    /// The target the corridor was searched for.
    path_for: Vec3,
    /// The navmesh's build counter when the corridor was made.
    path_made: u64,
    path_age: f32,
    need_path: bool,
    pub corner: Option<Vec3>,
    /// Path length still to go, metres (0 when idle).
    pub remaining: f32,
    /// Seconds moving without getting anywhere.
    pub stuck_s: f32,
    pub repaths: u64,
    /// How its path searches weigh areas.
    pub filter: Filter,
    /// Crossing an off-mesh link (jumping, dropping, climbing) right now.
    pub crossing: Option<Crossing>,
}

/// An agent on its way across an off-mesh link.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crossing {
    pub from: Vec3,
    pub to: Vec3,
    pub kind: LinkKind,
    /// Seconds since it took off, and how long the crossing takes.
    pub t: f32,
    pub duration: f32,
    /// Where it lands.
    pub to_poly: PolyRef,
    pub link: u32,
}

impl Crossing {
    /// Where the crossing is at `u` (0..1): a straight line, raised in an arc for a jump.
    pub fn at(&self, u: f32) -> Vec3 {
        let u = u.clamp(0.0, 1.0);
        let p = self.from + (self.to - self.from) * u;
        match self.kind {
            LinkKind::Jump => {
                let lift = (horizontal_distance(self.from, self.to) * 0.25).clamp(0.3, 2.0);
                p + Vec3::new(0.0, 4.0 * lift * u * (1.0 - u), 0.0)
            }
            LinkKind::Drop | LinkKind::Ladder => p,
        }
    }
}

impl CrowdAgent {
    fn new(params: AgentParams, pos: Vec3) -> CrowdAgent {
        CrowdAgent {
            params,
            pos,
            vel: Vec3::ZERO,
            desired: Vec3::ZERO,
            poly: None,
            poly_at: 0,
            target: None,
            status: AgentStatus::Idle,
            corridor: Vec::new(),
            path_end: pos,
            path_for: pos,
            path_made: 0,
            path_age: 0.0,
            need_path: false,
            corner: None,
            remaining: 0.0,
            stuck_s: 0.0,
            repaths: 0,
            filter: Filter::default(),
            crossing: None,
        }
    }

    pub fn to_json(&self) -> Json {
        let r = |v: f32| Json::from(specula_core::dmath::round_f64(f64::from(v) * 1000.0) / 1000.0);
        let v = |p: Vec3| Json::arr([r(p.x), r(p.y), r(p.z)]);
        Json::obj([
            ("status", Json::from(self.status.name())),
            ("position", v(self.pos)),
            ("velocity", v(self.vel)),
            ("target", self.target.map_or(Json::Null, v)),
            ("corner", self.corner.map_or(Json::Null, v)),
            ("remaining", r(self.remaining)),
            ("corridor", Json::from(self.corridor.len())),
            ("stuck_s", r(self.stuck_s)),
            ("repaths", Json::from(self.repaths)),
            ("crossing", self.crossing.map_or(Json::Null, |c| Json::obj([("kind", Json::from(c.kind.name())), ("to", v(c.to)), ("progress", r(c.t / c.duration.max(1.0e-3)))]))),
        ])
    }

    /// Every field, exactly (floats as their bits): [`Crowd::restore`] reads it back.
    fn save(&self) -> Json {
        let f = |v: f32| Json::from(v.to_bits());
        let v = |p: Vec3| Json::arr([f(p.x), f(p.y), f(p.z)]);
        let pr = &self.params;
        Json::obj([
            ("params", Json::arr([f(pr.radius), f(pr.speed), f(pr.accel), f(pr.arrive), Json::from(pr.avoid), Json::from(pr.external)])),
            ("pos", v(self.pos)),
            ("vel", v(self.vel)),
            ("desired", v(self.desired)),
            ("poly", self.poly.map_or(Json::Null, |p| Json::from(p.0))),
            ("poly_at", Json::from(self.poly_at)),
            ("target", self.target.map_or(Json::Null, v)),
            ("status", Json::from(self.status as u32)),
            ("corridor", Json::arr(self.corridor.iter().map(|p| Json::from(p.0)))),
            ("path_end", v(self.path_end)),
            ("path_for", v(self.path_for)),
            ("path_made", Json::from(self.path_made)),
            ("path_age", f(self.path_age)),
            ("need_path", Json::from(self.need_path)),
            ("corner", self.corner.map_or(Json::Null, v)),
            ("remaining", f(self.remaining)),
            ("stuck_s", f(self.stuck_s)),
            ("repaths", Json::from(self.repaths)),
            ("filter", Json::arr(self.filter.cost.iter().map(|c| f(*c)).chain([Json::from(self.filter.exclude)]))),
            (
                "crossing",
                self.crossing.map_or(Json::Null, |c| {
                    Json::arr([v(c.from), v(c.to), Json::from(c.kind as u32), f(c.t), f(c.duration), Json::from(c.to_poly.0), Json::from(c.link)])
                }),
            ),
        ])
    }

    fn restore(j: &Json) -> Option<CrowdAgent> {
        let f = |j: &Json| j.as_u64().and_then(|b| u32::try_from(b).ok()).map(f32::from_bits);
        let v = |j: &Json| {
            let a = j.as_array()?;
            Some(Vec3::new(f(a.first()?)?, f(a.get(1)?)?, f(a.get(2)?)?))
        };
        let opt_v = |k: &str| match j.get(k) {
            None | Some(Json::Null) => Some(None),
            Some(x) => v(x).map(Some),
        };
        let poly = |x: &Json| x.as_u64().and_then(|b| u32::try_from(b).ok()).map(PolyRef);
        let p = j.get("params")?.as_array()?;
        let params = AgentParams {
            radius: f(p.first()?)?,
            speed: f(p.get(1)?)?,
            accel: f(p.get(2)?)?,
            arrive: f(p.get(3)?)?,
            avoid: p.get(4)?.as_bool()?,
            external: p.get(5)?.as_bool()?,
        };
        let status = match j.get("status")?.as_u64()? {
            0 => AgentStatus::Idle,
            1 => AgentStatus::Waiting,
            2 => AgentStatus::Moving,
            3 => AgentStatus::Arrived,
            4 => AgentStatus::Partial,
            _ => AgentStatus::OffMesh,
        };
        let fl = j.get("filter")?.as_array()?;
        let mut filter = Filter::default();
        for (k, c) in filter.cost.iter_mut().enumerate() {
            *c = f(fl.get(k)?)?;
        }
        filter.exclude = u32::try_from(fl.get(crate::MAX_AREAS)?.as_u64()?).ok()?;
        let crossing = match j.get("crossing") {
            None | Some(Json::Null) => None,
            Some(c) => {
                let c = c.as_array()?;
                let kind = match c.get(2)?.as_u64()? {
                    0 => LinkKind::Jump,
                    1 => LinkKind::Drop,
                    _ => LinkKind::Ladder,
                };
                Some(Crossing { from: v(c.first()?)?, to: v(c.get(1)?)?, kind, t: f(c.get(3)?)?, duration: f(c.get(4)?)?, to_poly: poly(c.get(5)?)?, link: u32::try_from(c.get(6)?.as_u64()?).ok()? })
            }
        };
        Some(CrowdAgent {
            params,
            pos: v(j.get("pos")?)?,
            vel: v(j.get("vel")?)?,
            desired: v(j.get("desired")?)?,
            poly: match j.get("poly") {
                None | Some(Json::Null) => None,
                Some(x) => Some(poly(x)?),
            },
            poly_at: j.get("poly_at")?.as_u64()?,
            target: opt_v("target")?,
            status,
            corridor: j.get("corridor")?.as_array()?.iter().map(poly).collect::<Option<Vec<_>>>()?,
            path_end: v(j.get("path_end")?)?,
            path_for: v(j.get("path_for")?)?,
            path_made: j.get("path_made")?.as_u64()?,
            path_age: f(j.get("path_age")?)?,
            need_path: j.get("need_path")?.as_bool()?,
            corner: opt_v("corner")?,
            remaining: f(j.get("remaining")?)?,
            stuck_s: f(j.get("stuck_s")?)?,
            repaths: j.get("repaths")?.as_u64()?,
            filter,
            crossing,
        })
    }
}

/// What a step did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepStats {
    pub agents: usize,
    pub moving: usize,
    pub repaths: usize,
    /// Repath requests left for later steps.
    pub waiting: usize,
    pub nodes: usize,
    /// Neighbour candidates looked at (avoidance and push-apart).
    pub scanned: usize,
}

/// Agents on one navmesh.
#[derive(Clone, Debug)]
pub struct Crowd {
    pub settings: CrowdSettings,
    agents: BTreeMap<u64, CrowdAgent>,
    /// Unit directions of the velocity samples, built once with `dmath`.
    table: [(f32, f32); DIRECTIONS],
    /// The last agent whose repath was served (round robin).
    last_served: u64,
    scratch: Scratch,
}

impl Crowd {
    pub fn new(settings: CrowdSettings) -> Crowd {
        let table = std::array::from_fn(|k| {
            let a = k as f32 * (std::f32::consts::TAU / DIRECTIONS as f32);
            let (s, c) = dmath::sin_cos(a);
            (c, s)
        });
        Crowd { settings, agents: BTreeMap::new(), table, last_served: 0, scratch: Scratch::default() }
    }

    pub fn len(&self) -> usize {
        self.agents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    pub fn agents(&self) -> impl Iterator<Item = (u64, &CrowdAgent)> + '_ {
        self.agents.iter().map(|(k, a)| (*k, a))
    }

    pub fn agent(&self, id: u64) -> Option<&CrowdAgent> {
        self.agents.get(&id)
    }

    /// Adds an agent (or updates its parameters, keeping its state).
    pub fn upsert(&mut self, id: u64, params: AgentParams, pos: Vec3) -> Result<(), NavError> {
        if let Some(a) = self.agents.get_mut(&id) {
            a.params = params;
            return Ok(());
        }
        if self.agents.len() >= MAX_AGENTS {
            return Err(NavError::new("budget", format!("a crowd holds at most {MAX_AGENTS} agents")).with("id", id));
        }
        self.agents.insert(id, CrowdAgent::new(params, pos));
        Ok(())
    }

    pub fn remove(&mut self, id: u64) {
        self.agents.remove(&id);
    }

    pub fn ids(&self) -> Vec<u64> {
        self.agents.keys().copied().collect()
    }

    /// Sets (or clears) an agent's target.
    pub fn set_target(&mut self, id: u64, target: Option<Vec3>) {
        if let Some(a) = self.agents.get_mut(&id) {
            if a.target != target {
                a.target = target;
                if target.is_none() {
                    a.corridor.clear();
                    a.status = AgentStatus::Idle;
                    a.need_path = false;
                    a.corner = None;
                    a.remaining = 0.0;
                } else if a.status == AgentStatus::Idle || a.status == AgentStatus::Arrived {
                    a.status = AgentStatus::Waiting;
                }
            }
        }
    }

    /// How an agent's path searches weigh areas (a change searches again).
    pub fn set_filter(&mut self, id: u64, filter: Filter) {
        if let Some(a) = self.agents.get_mut(&id) {
            if a.filter != filter {
                a.filter = filter;
                a.need_path = true;
            }
        }
    }

    /// The crowd's whole state, exactly: every agent and the repath round robin. Polygon refs
    /// are only meaningful on the same navmesh (the caller checks its hash).
    pub fn save(&self) -> Json {
        Json::obj([
            ("last_served", Json::from(self.last_served)),
            ("agents", Json::arr(self.agents.iter().map(|(id, a)| Json::obj([("id", Json::from(*id)), ("agent", a.save())])))),
        ])
    }

    /// Restores [`Crowd::save`]'s state (settings stay as they are).
    pub fn restore(&mut self, j: &Json) -> Result<(), NavError> {
        let bad = |what: &str| NavError::new("invalid_save", format!("the crowd's saved state is damaged ({what})"));
        let last = j.get("last_served").and_then(Json::as_u64).ok_or_else(|| bad("last_served"))?;
        let mut agents = BTreeMap::new();
        for a in j.get("agents").and_then(Json::as_array).ok_or_else(|| bad("agents"))? {
            let id = a.get("id").and_then(Json::as_u64).ok_or_else(|| bad("id"))?;
            let st = a.get("agent").and_then(CrowdAgent::restore).ok_or_else(|| bad("an agent"))?;
            agents.insert(id, st);
        }
        if agents.len() > MAX_AGENTS {
            return Err(bad("too many agents"));
        }
        self.agents = agents;
        self.last_served = last;
        Ok(())
    }

    /// Every agent searches its path again (off-mesh links changed).
    pub fn invalidate_paths(&mut self) {
        for a in self.agents.values_mut() {
            if a.target.is_some() {
                a.need_path = true;
            }
        }
    }

    /// Where an externally moved agent is now (before the next step).
    pub fn set_position(&mut self, id: u64, pos: Vec3) {
        if let Some(a) = self.agents.get_mut(&id) {
            a.pos = pos;
        }
    }

    /// One fixed step of `dt` seconds.
    pub fn step(&mut self, mesh: &NavMesh, dt: f32) -> StepStats {
        let s = self.settings;
        let mut stats = StepStats { agents: self.agents.len(), ..StepStats::default() };
        let counter = mesh.counter();
        // 1. Polygons and corridors (an agent crossing a link has neither until it lands).
        for a in self.agents.values_mut() {
            if a.crossing.is_some() {
                continue;
            }
            let valid = a.poly.and_then(|p| mesh.poly(p)).is_some_and(|p| {
                mesh.contains_xz(p, a.pos) && (mesh.height(p, a.pos.x, a.pos.z) - a.pos.y).abs() <= s.extent.y
            }) && a.poly.is_some_and(|p| !mesh.rebuilt_since(p.tile(), a.poly_at));
            if !valid {
                a.poly = mesh.nearest(a.pos, s.extent).map(|(p, _)| p);
                a.poly_at = counter;
            }
            let Some(poly) = a.poly else {
                a.status = AgentStatus::OffMesh;
                a.corner = None;
                continue;
            };
            let Some(target) = a.target else {
                a.status = AgentStatus::Idle;
                continue;
            };
            a.path_age += dt;
            if let Some(k) = a.corridor.iter().take(8).position(|p| *p == poly) {
                a.corridor.drain(..k);
            } else if !a.corridor.is_empty() {
                a.need_path = true;
            }
            let stale_tiles = counter > a.path_made && a.corridor.iter().any(|p| mesh.rebuilt_since(p.tile(), a.path_made));
            if a.corridor.is_empty()
                || stale_tiles
                || a.path_age >= s.repath_s
                || horizontal_distance(target, a.path_for) > s.repath_distance
                || (target.y - a.path_for.y).abs() > s.extent.y
            {
                a.need_path = true;
            }
            if stale_tiles {
                // Refs into rebuilt tiles mean nothing now.
                a.corridor.clear();
            }
        }
        // 2. Path searches, round robin from the agent after the last one served.
        let mut order = std::mem::take(&mut self.scratch.order);
        order.clear();
        order.extend(self.agents.iter().filter(|(_, a)| a.need_path && a.poly.is_some() && a.target.is_some() && a.crossing.is_none()).map(|(k, _)| *k));
        let split = order.partition_point(|k| *k <= self.last_served);
        order.rotate_left(split);
        for (served, id) in order.iter().enumerate() {
            if served >= s.repaths_per_step || (served > 0 && stats.nodes >= s.nodes_per_step) {
                stats.waiting = order.len() - served;
                break;
            }
            let a = self.agents.get_mut(id).expect("listed");
            let (poly, target) = (a.poly.expect("filtered"), a.target.expect("filtered"));
            let goal = mesh.nearest(target, s.extent).filter(|(p, _)| mesh.poly(*p).is_some_and(|q| a.filter.cost_of(q.area).is_some()));
            let end = goal.map_or(target, |(_, e)| e);
            let (polys, status, visited) = mesh.search_filtered(poly, a.pos, goal.map(|(p, _)| p), end, s.max_nodes, &a.filter);
            stats.nodes += visited;
            stats.repaths += 1;
            let complete = status == crate::query::PathStatus::Complete && polys.last() == goal.map(|(p, _)| p).as_ref();
            a.path_end = if complete {
                end
            } else {
                polys.last().and_then(|p| mesh.poly(*p)).map_or(a.pos, |p| mesh.closest_on_poly(p, end))
            };
            let mut polys = mesh.shortcut_filtered(a.pos, a.path_end, polys, &a.filter);
            let complete = complete && polys.len() <= s.max_corridor;
            if polys.len() > s.max_corridor {
                // A long way: walk the first part, then search again from there.
                polys.truncate(s.max_corridor);
                a.path_end = polys.last().and_then(|p| mesh.poly(*p)).map_or(a.pos, |p| mesh.closest_on_poly(p, end));
            }
            a.corridor = polys;
            a.path_for = target;
            a.path_made = counter;
            a.path_age = 0.0;
            a.need_path = false;
            a.repaths += 1;
            a.status = if complete { AgentStatus::Moving } else { AgentStatus::Partial };
            self.last_served = *id;
        }
        self.scratch.order = order;
        // 3. Steering toward the next corner.
        for a in self.agents.values_mut() {
            a.desired = Vec3::ZERO;
            if a.crossing.is_some() {
                continue;
            }
            if a.poly.is_none() || a.target.is_none() {
                a.corner = None;
                continue;
            }
            if a.corridor.is_empty() {
                a.status = AgentStatus::Waiting;
                a.corner = None;
                continue;
            }
            // At an off-mesh link's take-off point: start across it.
            // (Only the corridor's first step matters here: the polygon it stands in.)
            let next_hop = a.corridor.get(..2).and_then(|w| mesh.traverse_between(w[0], w[1]));
            if let Some(t) = next_hop {
                if horizontal_distance(a.pos, t.from_pos) <= a.params.arrive.max(a.params.radius) && (a.pos.y - t.from_pos.y).abs() <= s.extent.y {
                    let speed = (a.params.speed * t.kind.speed_share()).max(0.1);
                    let duration = ((t.to_pos - a.pos).length() / speed).max(0.2);
                    a.crossing = Some(Crossing { from: a.pos, to: t.to_pos, kind: t.kind, t: 0.0, duration, to_poly: t.to, link: t.link });
                    a.corridor.drain(..1);
                    a.corner = Some(t.to_pos);
                    a.status = AgentStatus::Moving;
                    continue;
                }
            }
            // The next few corners; past them the rest counts as a straight line (exact on the
            // last stretch, where arrival slows the agent).
            let corners = mesh.straighten(a.pos, a.path_end, &a.corridor, 3);
            let last = *corners.last().expect("start");
            a.remaining = corners.windows(2).map(|w| (w[0] - w[1]).length()).sum::<f32>() + (a.path_end - last).length();
            let to_end = horizontal_distance(a.pos, a.path_end);
            if to_end <= a.params.arrive.max(1.0e-3) && mesh.first_traverse(&a.corridor).is_none() {
                a.corner = None;
                a.remaining = 0.0;
                if a.status == AgentStatus::Moving {
                    a.status = AgentStatus::Arrived;
                }
                continue;
            }
            if a.status == AgentStatus::Arrived || a.status == AgentStatus::Waiting {
                a.status = AgentStatus::Moving;
            }
            let corner = corners.get(1).copied().unwrap_or(a.path_end);
            a.corner = Some(corner);
            let (dx, dz) = (corner.x - a.pos.x, corner.z - a.pos.z);
            let d = (dx * dx + dz * dz).sqrt();
            if d > 1.0e-6 {
                let slow = (a.params.arrive * 3.0).max(0.5);
                let speed = a.params.speed * (a.remaining / slow).min(1.0);
                a.desired = Vec3::new(dx / d * speed, 0.0, dz / d * speed);
            }
        }
        // 4. Avoidance: neighbours from a grid (agents sorted by cell), sampled velocities. The
        // step works on a snapshot in id order, in buffers kept between steps.
        let mut sc = std::mem::take(&mut self.scratch);
        sc.snap.clear();
        sc.snap.extend(self.agents.iter().map(|(id, a)| Snap {
            id: *id,
            pos: a.pos,
            vel: a.vel,
            radius: a.params.radius,
            reciprocal: a.params.avoid && a.target.is_some(),
            on_mesh: a.poly.is_some() && a.crossing.is_none(),
        }));
        let cell = |p: Vec3| (dmath::floor(p.z / GRID) as i32, dmath::floor(p.x / GRID) as i32);
        sc.grid.clear();
        sc.grid.extend(sc.snap.iter().enumerate().filter(|(_, a)| a.on_mesh).map(|(i, a)| (cell(a.pos), i as u32)));
        sc.grid.sort_unstable();
        sc.chosen.clear();
        for (i, a) in self.agents.values().enumerate() {
            if a.poly.is_none() || a.crossing.is_some() {
                sc.chosen.push(Vec3::ZERO);
                continue;
            }
            sc.near.clear();
            let (cz, cx) = cell(a.pos);
            for gz in cz - 1..=cz + 1 {
                let lo = sc.grid.partition_point(|(c, _)| *c < (gz, cx - 1));
                let hi = sc.grid.partition_point(|(c, _)| *c <= (gz, cx + 1));
                // (A clump looks at its first MAX_SCAN agents in grid order, not all of them.)
                for &(_, j) in sc.grid[lo..hi].iter().take(MAX_SCAN) {
                    stats.scanned += 1;
                    if j as usize == i {
                        continue;
                    }
                    let d = horizontal_distance(a.pos, sc.snap[j as usize].pos);
                    if d < GRID {
                        sc.near.push((d, j));
                    }
                }
            }
            // The nearest few (distance, then id: a total order, so the same set every run).
            let snap = &sc.snap;
            let order = |x: &(f32, u32), y: &(f32, u32)| x.0.total_cmp(&y.0).then(snap[x.1 as usize].id.cmp(&snap[y.1 as usize].id));
            if sc.near.len() > s.neighbours && s.neighbours > 0 {
                sc.near.select_nth_unstable_by(s.neighbours - 1, order);
            }
            sc.near.truncate(s.neighbours);
            sc.near.sort_by(order);
            let v = if a.params.avoid && !sc.near.is_empty() && a.desired != Vec3::ZERO {
                sc.others.clear();
                sc.others.extend(sc.near.iter().map(|(_, j)| {
                    let b = &sc.snap[*j as usize];
                    Other { pos: b.pos, vel: b.vel, radius: b.radius, reciprocal: b.reciprocal }
                }));
                self.sample(a, &sc.others, &s)
            } else {
                a.desired
            };
            sc.chosen.push(v);
        }
        // 5. Move (crossing agents along their link: position written for external ones too,
        // since nothing else can carry them across).
        for ((_, a), v) in self.agents.iter_mut().zip(sc.chosen.iter().copied()) {
            if let Some(mut c) = a.crossing {
                c.t += dt;
                let u = c.t / c.duration;
                let p = c.at(u);
                a.vel = (c.to - c.from) * (1.0 / c.duration);
                a.pos = p;
                stats.moving += 1;
                if u >= 1.0 {
                    a.pos = c.to;
                    a.poly = Some(c.to_poly);
                    a.poly_at = counter;
                    a.crossing = None;
                    a.vel = Vec3::ZERO;
                } else {
                    a.crossing = Some(c);
                }
                continue;
            }
            let dv = v - a.vel;
            let max_dv = a.params.accel * dt;
            let len = dv.length();
            a.vel = if len > max_dv && len > 0.0 { a.vel + dv * (max_dv / len) } else { v };
            if a.desired == Vec3::ZERO && a.vel.length() < 0.05 {
                a.vel = Vec3::ZERO;
            }
            if a.vel != Vec3::ZERO {
                stats.moving += 1;
            }
            let Some(poly) = a.poly else { continue };
            if a.params.external {
                continue;
            }
            let before = a.pos;
            let (np, p) = mesh.move_along_filtered(poly, a.pos, a.pos + a.vel * dt, &a.filter);
            a.poly = Some(np);
            a.pos = p;
            let moved = horizontal_distance(before, p);
            if matches!(a.status, AgentStatus::Moving | AgentStatus::Partial) && moved < 0.1 * a.params.speed * dt {
                a.stuck_s += dt;
            } else {
                a.stuck_s = 0.0;
            }
        }
        // 6. Push overlapping agents apart (crowd-moved ones only), each pair once, from the
        // positions after moving.
        for (snap, a) in sc.snap.iter_mut().zip(self.agents.values()) {
            snap.pos = a.pos;
        }
        sc.grid.clear();
        sc.grid.extend(sc.snap.iter().enumerate().filter(|(_, a)| a.on_mesh).map(|(i, a)| (cell(a.pos), i as u32)));
        sc.grid.sort_unstable();
        sc.push.clear();
        sc.push.resize(sc.snap.len(), Vec3::ZERO);
        for i in 0..sc.snap.len() {
            let a = sc.snap[i];
            if !a.on_mesh {
                continue;
            }
            let (cz, cx) = cell(a.pos);
            for gz in cz - 1..=cz + 1 {
                let lo = sc.grid.partition_point(|(c, _)| *c < (gz, cx - 1));
                let hi = sc.grid.partition_point(|(c, _)| *c <= (gz, cx + 1));
                for k in lo..hi.min(lo + MAX_SCAN) {
                    stats.scanned += 1;
                    let j = sc.grid[k].1 as usize;
                    if j <= i {
                        continue;
                    }
                    let b = sc.snap[j];
                    let r = a.radius + b.radius;
                    let (dx, dz) = (a.pos.x - b.pos.x, a.pos.z - b.pos.z);
                    if dx.abs() >= r || dz.abs() >= r {
                        continue;
                    }
                    let d = (dx * dx + dz * dz).sqrt();
                    if d >= r {
                        continue;
                    }
                    let (nx, nz) = if d > 1.0e-5 { (dx / d, dz / d) } else { (1.0, 0.0) };
                    let overlap = (r - d) * 0.5;
                    sc.push[i] = sc.push[i] + Vec3::new(nx * overlap, 0.0, nz * overlap);
                    sc.push[j] = sc.push[j] - Vec3::new(nx * overlap, 0.0, nz * overlap);
                }
            }
        }
        for ((_, a), off) in self.agents.iter_mut().zip(sc.push.iter().copied()) {
            if a.params.external || off == Vec3::ZERO || a.crossing.is_some() {
                continue;
            }
            if let Some(poly) = a.poly {
                let (np, p) = mesh.move_along_filtered(poly, a.pos, a.pos + off, &a.filter);
                a.poly = Some(np);
                a.pos = p;
            }
        }
        self.scratch = sc;
        stats
    }

    /// The best of the sampled velocities for agent `a` among its neighbours.
    fn sample(&self, a: &CrowdAgent, near: &[Other], s: &CrowdSettings) -> Vec3 {
        let vmax = a.params.speed.max(1.0e-3);
        let dl = (a.desired.x * a.desired.x + a.desired.z * a.desired.z).sqrt();
        let (fx, fz) = if dl > 1.0e-6 { (a.desired.x / dl, a.desired.z / dl) } else { (1.0, 0.0) };
        let mut candidates = [Vec3::ZERO; DIRECTIONS * SPEEDS.len() + 2];
        candidates[0] = a.desired;
        for (si, speed) in SPEEDS.iter().enumerate() {
            for (k, (c, sn)) in self.table.iter().enumerate() {
                // Rotate the table direction by the desired direction.
                let (x, z) = (fx * c - fz * sn, fz * c + fx * sn);
                candidates[1 + si * DIRECTIONS + k] = Vec3::new(x * vmax * speed, 0.0, z * vmax * speed);
            }
        }
        // The last stays zero: standing still.
        let mut best = (f32::INFINITY, a.desired);
        for v in candidates {
            let mut tmin = s.horizon_s;
            for b in near {
                let vab = if b.reciprocal { v * 2.0 - a.vel - b.vel } else { v - b.vel };
                let (px, pz) = (a.pos.x - b.pos.x, a.pos.z - b.pos.z);
                let r = a.params.radius + b.radius;
                let (vx, vz) = (vab.x, vab.z);
                let c = px * px + pz * pz - r * r;
                let bq = px * vx + pz * vz;
                if c < 0.0 {
                    // Already overlapping: only velocities that separate are fine.
                    if bq < 0.0 {
                        tmin = 0.0;
                    }
                    continue;
                }
                let aq = vx * vx + vz * vz;
                if aq < 1.0e-9 {
                    continue;
                }
                let disc = bq * bq - aq * c;
                if disc < 0.0 {
                    continue;
                }
                let t = (-bq - disc.sqrt()) / aq;
                if t >= 0.0 && t < tmin {
                    tmin = t;
                }
            }
            let pen_des = (v - a.desired).length() / vmax;
            let pen_cur = (v - a.vel).length() / vmax;
            let pen_toi = if tmin < s.horizon_s { 1.0 / (0.1 + tmin / s.horizon_s) } else { 0.0 };
            let score = 2.0 * pen_des + 0.75 * pen_cur + 2.5 * pen_toi;
            if score < best.0 {
                best = (score, v);
            }
        }
        best.1
    }

    /// Hash of every agent's state, by id.
    pub fn hash(&self) -> u64 {
        let mut h = StableHasher::default();
        h.write_u64(self.agents.len() as u64);
        h.write_u64(self.last_served);
        let v = |h: &mut StableHasher, p: Vec3| {
            h.write_f32(p.x);
            h.write_f32(p.y);
            h.write_f32(p.z);
        };
        for (id, a) in &self.agents {
            h.write_u64(*id);
            v(&mut h, a.pos);
            v(&mut h, a.vel);
            h.write_u32(a.poly.map_or(u32::MAX, |p| p.0));
            match a.target {
                Some(t) => v(&mut h, t),
                None => h.write_u8(0),
            }
            h.write_u8(a.status as u8);
            h.write_u64(a.corridor.len() as u64);
            for p in &a.corridor {
                h.write_u32(p.0);
            }
            v(&mut h, a.path_end);
            h.write_f32(a.path_age);
            h.write_bool(a.need_path);
            h.write_f32(a.stuck_s);
            a.filter.hash_into(&mut h);
            match a.crossing {
                Some(c) => {
                    h.write_u8(1 + c.kind as u8);
                    h.write_f32(c.t);
                    h.write_f32(c.duration);
                    v(&mut h, c.to);
                    h.write_u32(c.to_poly.0);
                }
                None => h.write_u8(0),
            }
        }
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::tests::{build_all, room_config, scene};
    use crate::Source;

    fn mesh() -> NavMesh {
        let mut m = NavMesh::new(room_config()).unwrap();
        build_all(&mut m, &scene([-10.0, -10.0, 10.0, 10.0], &[(Vec3::new(-0.25, 0.0, -10.0), Vec3::new(0.25, 2.0, 6.0))]));
        m
    }

    fn run(crowd: &mut Crowd, m: &NavMesh, steps: usize) {
        for _ in 0..steps {
            crowd.step(m, 1.0 / 30.0);
        }
    }

    #[test]
    fn an_agent_walks_around_the_wall_and_arrives() {
        let m = mesh();
        let mut c = Crowd::new(CrowdSettings::default());
        c.upsert(1, AgentParams::default(), Vec3::new(-5.0, 0.0, 0.0)).unwrap();
        c.set_target(1, Some(Vec3::new(5.0, 0.0, 0.0)));
        let mut max_z = 0.0f32;
        for _ in 0..400 {
            c.step(&m, 1.0 / 30.0);
            let a = c.agent(1).unwrap();
            max_z = max_z.max(a.pos.z);
            // Never inside the wall (with its radius).
            assert!(!(a.pos.x.abs() < 0.25 + 0.29 && a.pos.z < 6.0), "in the wall at {:?}", a.pos);
        }
        let a = c.agent(1).unwrap();
        assert_eq!(a.status, AgentStatus::Arrived, "{a:?}");
        assert!(horizontal_distance(a.pos, Vec3::new(5.0, 0.0, 0.0)) <= 0.3, "{:?}", a.pos);
        assert!(max_z > 6.0, "went around the wall's end");
    }

    #[test]
    fn agents_crossing_avoid_each_other() {
        let m = NavMesh::new(room_config()).map(|mut m| {
            build_all(&mut m, &scene([-10.0, -10.0, 10.0, 10.0], &[]));
            m
        });
        let m = m.unwrap();
        let mut c = Crowd::new(CrowdSettings::default());
        for i in 0..6u64 {
            let z = -2.5 + i as f32;
            let (from, to) = if i % 2 == 0 { (-6.0, 6.0) } else { (6.0, -6.0) };
            c.upsert(i + 1, AgentParams::default(), Vec3::new(from, 0.0, z)).unwrap();
            c.set_target(i + 1, Some(Vec3::new(to, 0.0, z)));
        }
        let mut closest = f32::INFINITY;
        for _ in 0..360 {
            c.step(&m, 1.0 / 30.0);
            let all: Vec<Vec3> = c.agents().map(|(_, a)| a.pos).collect();
            for i in 0..all.len() {
                for j in i + 1..all.len() {
                    closest = closest.min(horizontal_distance(all[i], all[j]));
                }
            }
        }
        assert!(closest > 0.45, "agents stay mostly apart (two radii = 0.6): closest {closest}");
        let arrived = c.agents().filter(|(_, a)| a.status == AgentStatus::Arrived).count();
        assert_eq!(arrived, 6, "{:?}", c.agents().map(|(_, a)| (a.status, a.pos)).collect::<Vec<_>>());
    }

    #[test]
    fn a_head_on_pair_passes_and_arrives() {
        let m = NavMesh::new(room_config()).map(|mut m| {
            build_all(&mut m, &scene([-10.0, -10.0, 10.0, 10.0], &[]));
            m
        });
        let m = m.unwrap();
        let mut c = Crowd::new(CrowdSettings::default());
        c.upsert(1, AgentParams::default(), Vec3::new(-4.0, 0.0, 0.0)).unwrap();
        c.upsert(2, AgentParams::default(), Vec3::new(4.0, 0.0, 0.0)).unwrap();
        c.set_target(1, Some(Vec3::new(4.0, 0.0, 0.0)));
        c.set_target(2, Some(Vec3::new(-4.0, 0.0, 0.0)));
        let mut closest = f32::INFINITY;
        for _ in 0..240 {
            c.step(&m, 1.0 / 30.0);
            closest = closest.min(horizontal_distance(c.agent(1).unwrap().pos, c.agent(2).unwrap().pos));
        }
        assert!(closest > 0.5, "they pass each other (closest {closest})");
        assert!(c.agents().all(|(_, a)| a.status == AgentStatus::Arrived), "{:?}", c.agents().map(|(_, a)| (a.status, a.pos)).collect::<Vec<_>>());
    }

    #[test]
    fn a_clump_at_one_point_costs_a_bounded_scan() {
        let m = NavMesh::new(room_config()).map(|mut m| {
            build_all(&mut m, &scene([-10.0, -10.0, 10.0, 10.0], &[]));
            m
        });
        let m = m.unwrap();
        let make = || {
            let mut c = Crowd::new(CrowdSettings::default());
            for i in 0..1024u64 {
                c.upsert(i + 1, AgentParams::default(), Vec3::new(0.0, 0.0, 0.0)).unwrap();
                c.set_target(i + 1, Some(Vec3::new(if i % 2 == 0 { 8.0 } else { -8.0 }, 0.0, 0.0)));
            }
            c
        };
        let mut a = make();
        let s = a.step(&m, 1.0 / 30.0);
        assert!(s.scanned <= 1024 * 6 * MAX_SCAN, "{}", s.scanned);
        let mut b = make();
        b.step(&m, 1.0 / 30.0);
        run(&mut a, &m, 30);
        run(&mut b, &m, 30);
        assert_eq!(a.hash(), b.hash(), "still deterministic");
    }

    /// Two floors with a 2 m gap between them (x -10..-1 and 1..10), a jump link across it.
    fn gap_mesh(bidirectional: bool, kind: LinkKind) -> NavMesh {
        let mut m = NavMesh::new(room_config()).unwrap();
        let floors = vec![Source::new(crate::mesh::tests::quad(-10.0, -10.0, -1.0, 10.0, 0.0), false), Source::new(crate::mesh::tests::quad(1.0, -10.0, 10.0, 10.0, 0.0), false)];
        build_all(&mut m, &floors);
        m.set_offmesh_links(vec![crate::OffMeshLink {
            from: Vec3::new(-1.5, 0.0, 0.0),
            to: Vec3::new(1.5, 0.0, 0.0),
            kind,
            bidirectional,
            cost: 1.0,
            area: 0,
            radius: 0.5,
        }]);
        m
    }

    #[test]
    fn paths_and_agents_cross_off_mesh_links() {
        let m = gap_mesh(false, LinkKind::Jump);
        assert!(m.resolved_link(0).is_some());
        let p = m.find_path(Vec3::new(-5.0, 0.0, 3.0), Vec3::new(5.0, 0.0, -3.0), Vec3::new(1.0, 2.0, 1.0), 4096).unwrap();
        assert!(p.complete(), "{p:?}");
        assert_eq!(p.offmesh.len(), 1);
        let (k, kind) = p.offmesh[0];
        assert_eq!(kind, LinkKind::Jump);
        assert!(horizontal_distance(p.corners[k], Vec3::new(-1.5, 0.0, 0.0)) < 0.3 && horizontal_distance(p.corners[k + 1], Vec3::new(1.5, 0.0, 0.0)) < 0.3, "{:?}", p.corners);
        // One way: back across is unreachable.
        let back = m.find_path(Vec3::new(5.0, 0.0, 0.0), Vec3::new(-5.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 1.0), 4096).unwrap();
        assert!(!back.complete());
        // An agent jumps it (in an arc) and arrives.
        let mut c = Crowd::new(CrowdSettings::default());
        c.upsert(1, AgentParams::default(), Vec3::new(-5.0, 0.0, 3.0)).unwrap();
        c.set_target(1, Some(Vec3::new(5.0, 0.0, -3.0)));
        let (mut crossed, mut high) = (false, 0.0f32);
        for _ in 0..400 {
            c.step(&m, 1.0 / 30.0);
            let a = c.agent(1).unwrap();
            if a.crossing.is_some() {
                crossed = true;
                high = high.max(a.pos.y);
            }
        }
        let a = c.agent(1).unwrap();
        assert!(crossed && high > 0.2, "jumped in an arc ({high})");
        assert_eq!(a.status, AgentStatus::Arrived, "{a:?}");
    }

    #[test]
    fn area_costs_steer_paths_and_exclusions_wall_them_off() {
        // A floor whose middle strip (z -2..2 across the whole width but a 2 m gap at x 8..10) is water.
        let mut m = NavMesh::new(room_config()).unwrap();
        let floor = Source::new(crate::mesh::tests::quad(-10.0, -10.0, 10.0, 10.0, 0.0), false);
        let water = [crate::Mark { lo: Vec3::new(-10.0, -0.5, -2.0), hi: Vec3::new(8.0, 0.5, 2.0), area: 1 }];
        let (nx, nz) = m.tile_count();
        for tz in 0..nz {
            for tx in 0..nx {
                m.build_tile_from(tx, tz, &[crate::Input::all(&floor)], &water);
            }
        }
        let (from, to) = (Vec3::new(-5.0, 0.0, -6.0), Vec3::new(-5.0, 0.0, 6.0));
        let ext = Vec3::new(1.0, 2.0, 1.0);
        let plain = m.find_path(from, to, ext, 4096).unwrap();
        assert!(plain.complete() && plain.length < 13.0, "straight through: {}", plain.length);
        let mut costly = Filter::default();
        costly.cost[1] = 10.0;
        let around = m.find_path_filtered(from, to, ext, 4096, &costly).unwrap();
        assert!(around.complete() && around.length > 20.0, "around the water: {} {:?}", around.length, around.corners);
        assert!(around.corners.iter().any(|c| c.x > 7.5), "{:?}", around.corners);
        let dry = Filter { exclude: 1 << 1, ..Filter::default() };
        let walled = m.find_path_filtered(from, to, ext, 4096, &dry).unwrap();
        assert!(walled.complete() && walled.corners.iter().any(|c| c.x > 7.5), "{:?}", walled.corners);
        // An agent that may not swim walks around.
        let mut c = Crowd::new(CrowdSettings::default());
        c.upsert(1, AgentParams::default(), from).unwrap();
        c.set_filter(1, dry);
        c.set_target(1, Some(to));
        for _ in 0..600 {
            c.step(&m, 1.0 / 30.0);
            let a = c.agent(1).unwrap();
            let in_water = a.pos.x < 7.9 && a.pos.z > -1.9 && a.pos.z < 1.9;
            assert!(!in_water, "{:?}", a.pos);
        }
        assert_eq!(c.agent(1).unwrap().status, AgentStatus::Arrived);
    }

    #[test]
    fn a_restored_crowd_continues_exactly() {
        let m = gap_mesh(true, LinkKind::Jump);
        let make = || {
            let mut c = Crowd::new(CrowdSettings::default());
            for i in 0..12u64 {
                c.upsert(i + 1, AgentParams::default(), Vec3::new(-8.0 + (i % 4) as f32, 0.0, -6.0 + (i / 4) as f32 * 2.0)).unwrap();
                c.set_target(i + 1, Some(Vec3::new(6.0 - (i % 3) as f32, 0.0, 6.0 - (i % 5) as f32)));
            }
            c
        };
        let mut a = make();
        run(&mut a, &m, 75);
        let saved = a.save();
        let mut b = Crowd::new(CrowdSettings::default());
        b.restore(&Json::parse(&saved.to_string()).unwrap()).unwrap();
        assert_eq!(a.hash(), b.hash());
        for _ in 0..200 {
            a.step(&m, 1.0 / 30.0);
            b.step(&m, 1.0 / 30.0);
            assert_eq!(a.hash(), b.hash());
        }
        assert!(a.agents().any(|(_, x)| x.pos.x > 1.0), "some crossed");
        assert!(b.restore(&Json::parse(r#"{"agents": 3}"#).unwrap()).is_err());
    }

    #[test]
    fn repaths_are_budgeted_and_deterministic() {
        let m = mesh();
        let make = || {
            let mut c = Crowd::new(CrowdSettings { repaths_per_step: 4, ..CrowdSettings::default() });
            for i in 0..20u64 {
                c.upsert(i + 1, AgentParams::default(), Vec3::new(-8.0 + (i % 5) as f32, 0.0, -8.0 + (i / 5) as f32 * 1.5)).unwrap();
                c.set_target(i + 1, Some(Vec3::new(7.0, 0.0, 7.0 - (i % 4) as f32)));
            }
            c
        };
        let mut a = make();
        let first = a.step(&m, 1.0 / 30.0);
        assert_eq!(first.repaths, 4);
        assert_eq!(first.waiting, 16);
        let mut b = make();
        b.step(&m, 1.0 / 30.0);
        run(&mut a, &m, 120);
        run(&mut b, &m, 120);
        assert_eq!(a.hash(), b.hash());
    }
}

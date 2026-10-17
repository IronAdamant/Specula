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
use crate::NavError;
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

/// Crowd-wide settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrowdSettings {
    /// Path searches served per step.
    pub repaths_per_step: usize,
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
            max_corridor: 256,
            max_nodes: 4096,
            repath_s: 2.0,
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
        }
    }

    pub fn to_json(&self) -> Json {
        let r = |v: f32| Json::from((f64::from(v) * 1000.0).round() / 1000.0);
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
        ])
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
}

impl Crowd {
    pub fn new(settings: CrowdSettings) -> Crowd {
        let table = std::array::from_fn(|k| {
            let a = k as f32 * (std::f32::consts::TAU / DIRECTIONS as f32);
            let (s, c) = dmath::sin_cos(a);
            (c, s)
        });
        Crowd { settings, agents: BTreeMap::new(), table, last_served: 0 }
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
        // 1. Polygons and corridors.
        for a in self.agents.values_mut() {
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
            let stale_tiles = a.corridor.iter().any(|p| mesh.rebuilt_since(p.tile(), a.path_made));
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
        let mut order: Vec<u64> = self.agents.iter().filter(|(_, a)| a.need_path && a.poly.is_some() && a.target.is_some()).map(|(k, _)| *k).collect();
        let split = order.partition_point(|k| *k <= self.last_served);
        order.rotate_left(split);
        for (served, id) in order.iter().enumerate() {
            if served >= s.repaths_per_step {
                stats.waiting = order.len() - served;
                break;
            }
            let a = self.agents.get_mut(id).expect("listed");
            let (poly, target) = (a.poly.expect("filtered"), a.target.expect("filtered"));
            let goal = mesh.nearest(target, s.extent);
            let end = goal.map_or(target, |(_, e)| e);
            let (mut polys, status, visited) = mesh.search(poly, a.pos, goal.map(|(p, _)| p), end, s.max_nodes);
            stats.nodes += visited;
            stats.repaths += 1;
            polys.truncate(s.max_corridor);
            let complete = status == crate::query::PathStatus::Complete && polys.last() == goal.map(|(p, _)| p).as_ref();
            a.path_end = if complete {
                end
            } else {
                polys.last().and_then(|p| mesh.poly(*p)).map_or(a.pos, |p| mesh.closest_on_poly(p, end))
            };
            a.corridor = polys;
            a.path_for = target;
            a.path_made = counter;
            a.path_age = 0.0;
            a.need_path = false;
            a.repaths += 1;
            a.status = if complete { AgentStatus::Moving } else { AgentStatus::Partial };
            self.last_served = *id;
        }
        // 3. Steering toward the next corner.
        for a in self.agents.values_mut() {
            a.desired = Vec3::ZERO;
            if a.poly.is_none() || a.target.is_none() {
                a.corner = None;
                continue;
            }
            if a.corridor.is_empty() {
                a.status = AgentStatus::Waiting;
                a.corner = None;
                continue;
            }
            let corners = mesh.straighten(a.pos, a.path_end, &a.corridor, 24);
            a.remaining = corners.windows(2).map(|w| (w[0] - w[1]).length()).sum();
            let to_end = horizontal_distance(a.pos, a.path_end);
            if to_end <= a.params.arrive.max(1.0e-3) {
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
        // 4. Avoidance: neighbours from a grid, sampled velocities.
        let mut grid: BTreeMap<(i32, i32), Vec<u64>> = BTreeMap::new();
        let cell = |p: Vec3| (dmath::floor(p.x / GRID) as i32, dmath::floor(p.z / GRID) as i32);
        for (id, a) in &self.agents {
            if a.poly.is_some() {
                grid.entry(cell(a.pos)).or_default().push(*id);
            }
        }
        let mut chosen: Vec<(u64, Vec3)> = Vec::with_capacity(self.agents.len());
        for (id, a) in &self.agents {
            if a.poly.is_none() {
                chosen.push((*id, Vec3::ZERO));
                continue;
            }
            let mut near: Vec<(f32, u64)> = Vec::new();
            let (cx, cz) = cell(a.pos);
            for gz in cz - 1..=cz + 1 {
                for gx in cx - 1..=cx + 1 {
                    for other in grid.get(&(gx, gz)).into_iter().flatten() {
                        if other == id {
                            continue;
                        }
                        let b = &self.agents[other];
                        let d = horizontal_distance(a.pos, b.pos);
                        if d < GRID {
                            near.push((d, *other));
                        }
                    }
                }
            }
            near.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
            near.truncate(s.neighbours);
            let v = if a.params.avoid && !near.is_empty() && a.desired != Vec3::ZERO {
                self.sample(a, &near, &s)
            } else {
                a.desired
            };
            chosen.push((*id, v));
        }
        // 5. Move.
        for (id, v) in chosen {
            let a = self.agents.get_mut(&id).expect("listed");
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
            let (np, p) = mesh.move_along(poly, a.pos, a.pos + a.vel * dt);
            a.poly = Some(np);
            a.pos = p;
            let moved = horizontal_distance(before, p);
            if matches!(a.status, AgentStatus::Moving | AgentStatus::Partial) && moved < 0.1 * a.params.speed * dt {
                a.stuck_s += dt;
            } else {
                a.stuck_s = 0.0;
            }
        }
        // 6. Push overlapping agents apart (crowd-moved ones only).
        let ids: Vec<u64> = self.agents.keys().copied().collect();
        let mut push: BTreeMap<u64, Vec3> = BTreeMap::new();
        for (i, ia) in ids.iter().enumerate() {
            let a = &self.agents[ia];
            if a.poly.is_none() {
                continue;
            }
            for ib in &ids[i + 1..] {
                let b = &self.agents[ib];
                if b.poly.is_none() {
                    continue;
                }
                let r = a.params.radius + b.params.radius;
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
                let pa = push.entry(*ia).or_insert(Vec3::ZERO);
                *pa = *pa + Vec3::new(nx * overlap, 0.0, nz * overlap);
                let pb = push.entry(*ib).or_insert(Vec3::ZERO);
                *pb = *pb - Vec3::new(nx * overlap, 0.0, nz * overlap);
            }
        }
        for (id, off) in push {
            let a = self.agents.get_mut(&id).expect("listed");
            if a.params.external {
                continue;
            }
            if let Some(poly) = a.poly {
                let (np, p) = mesh.move_along(poly, a.pos, a.pos + off);
                a.poly = Some(np);
                a.pos = p;
            }
        }
        stats
    }

    /// The best of the sampled velocities for agent `a` among `near` neighbours.
    fn sample(&self, a: &CrowdAgent, near: &[(f32, u64)], s: &CrowdSettings) -> Vec3 {
        let vmax = a.params.speed.max(1.0e-3);
        let dl = (a.desired.x * a.desired.x + a.desired.z * a.desired.z).sqrt();
        let (fx, fz) = if dl > 1.0e-6 { (a.desired.x / dl, a.desired.z / dl) } else { (1.0, 0.0) };
        let mut candidates: Vec<Vec3> = Vec::with_capacity(DIRECTIONS * SPEEDS.len() + 2);
        candidates.push(a.desired);
        for speed in SPEEDS {
            for (c, sn) in self.table {
                // Rotate the table direction by the desired direction.
                let (x, z) = (fx * c - fz * sn, fz * c + fx * sn);
                candidates.push(Vec3::new(x * vmax * speed, 0.0, z * vmax * speed));
            }
        }
        candidates.push(Vec3::ZERO);
        let mut best = (f32::INFINITY, a.desired);
        for v in candidates {
            let mut tmin = s.horizon_s;
            for (_, other) in near {
                let b = &self.agents[other];
                let reciprocal = b.params.avoid && b.target.is_some();
                let vab = if reciprocal { v * 2.0 - a.vel - b.vel } else { v - b.vel };
                let (px, pz) = (a.pos.x - b.pos.x, a.pos.z - b.pos.z);
                let r = a.params.radius + b.params.radius;
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
        }
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::tests::{build_all, room_config, scene};

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
